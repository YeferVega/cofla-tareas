#![cfg(unix)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

const VERSION: &str = "9.9-test";
const FAIL_SAFE_BUDGET: Duration = Duration::from_secs(8);

struct Server {
    child: Child,
    stdout: BufReader<std::process::ChildStdout>,
    startup_line: String,
    port: u16,
}

fn spawn_server(extra_env: &[(&str, &str)]) -> Server {
    let mut command = Command::new(env!("CARGO_BIN_EXE_cofla-tareas-backend"));
    command
        .env("PORT", "0")
        .env("STORAGE", "memory")
        .env("APP_VERSION", VERSION)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let mut child = command.spawn().expect("server binary should start");
    let stdout = BufReader::new(child.stdout.take().expect("stdout should be piped"));
    let mut server = Server {
        child,
        stdout,
        startup_line: String::new(),
        port: 0,
    };
    server.read_startup_line();
    server
}

impl Server {
    fn read_startup_line(&mut self) {
        loop {
            let mut line = String::new();
            let read = self
                .stdout
                .read_line(&mut line)
                .expect("stdout should be readable");
            assert_ne!(read, 0, "server exited before printing the startup line");
            if line.starts_with("Cofla Tareas") {
                self.startup_line = line.trim().to_string();
                self.port = parse_port(&self.startup_line);
                return;
            }
        }
    }

    fn health_response(&mut self) -> String {
        let mut last_error = None;
        for _ in 0..20 {
            match one_shot_health(self.port) {
                Ok(response) => return response,
                Err(err) => last_error = Some(err),
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        panic!(
            "health request never succeeded: {}",
            last_error.expect("error is set on every failure")
        );
    }

    fn signal(&mut self, signal: i32) {
        let code = unsafe { libc::kill(self.child.id() as i32, signal) };
        assert_eq!(code, 0, "signal should be deliverable to the child");
    }

    fn wait_within_fail_safe(&mut self) -> ExitStatus {
        let started = Instant::now();
        loop {
            let status = self.child.try_wait().expect("try_wait should succeed");
            if let Some(status) = status {
                assert!(
                    started.elapsed() < FAIL_SAFE_BUDGET,
                    "server must finish draining within the 8s fail-safe budget"
                );
                return status;
            }
            assert!(
                started.elapsed() < FAIL_SAFE_BUDGET,
                "server ignored the shutdown signal past the fail-safe budget"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    fn drain_stderr(&mut self) -> String {
        let mut text = String::new();
        let _ = self.child.stderr.take().unwrap().read_to_string(&mut text);
        text
    }
}

fn parse_port(line: &str) -> u16 {
    let mut tokens = line.split_whitespace();
    while let Some(token) = tokens.next() {
        if token == "port" {
            return tokens
                .next()
                .unwrap_or_else(|| panic!("no port token in startup line: {line}"))
                .parse()
                .unwrap_or_else(|_| panic!("port token is not a number in: {line}"));
        }
    }
    panic!("startup line does not report a bound port: {line}");
}

fn one_shot_health(port: u16) -> std::io::Result<String> {
    let mut stream = TcpStream::connect(("127.0.0.1", port))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")?;
    stream.flush()?;
    let mut response = String::new();
    stream.read_to_string(&mut response)?;
    Ok(response)
}

#[test]
fn startup_log_reports_version_real_bound_port_and_storage_mode() {
    let mut server = spawn_server(&[]);
    let response = server.health_response();
    assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {response}");
    assert!(response.contains("\"storage\":\"memory\""));
    assert!(response.contains(&format!("\"version\":\"{VERSION}\"")));

    assert!(
        server.startup_line.contains(VERSION),
        "line: {}",
        server.startup_line
    );
    assert!(
        server.startup_line.contains("storage memory"),
        "line: {}",
        server.startup_line
    );
    assert!(
        server.port > 0,
        "startup line must report a real ephemeral port"
    );

    server.signal(libc::SIGTERM);
    let status = server.wait_within_fail_safe();
    assert!(status.success(), "exit: {status:?}");
}

#[test]
fn sigterm_stops_the_server_with_exit_code_zero() {
    let mut server = spawn_server(&[]);
    server.health_response();
    server.signal(libc::SIGTERM);
    let status = server.wait_within_fail_safe();
    assert!(
        status.success(),
        "exit: {status:?}, stderr: {}",
        server.drain_stderr()
    );
}

#[test]
fn sigint_stops_the_server_with_exit_code_zero() {
    let mut server = spawn_server(&[]);
    server.health_response();
    server.signal(libc::SIGINT);
    let status = server.wait_within_fail_safe();
    assert!(
        status.success(),
        "exit: {status:?}, stderr: {}",
        server.drain_stderr()
    );
}

#[test]
fn postgres_mode_startup_log_reports_storage_postgres() {
    let Some(url) = std::env::var("TEST_DATABASE_URL").ok() else {
        eprintln!("skipping: TEST_DATABASE_URL is not set");
        return;
    };
    let (host, port) = parse_host_port(&url);
    let mut server = spawn_server(&[
        ("STORAGE", "postgres"),
        ("PGHOST", host.as_str()),
        ("PGPORT", port.as_str()),
        ("PGUSER", "cofla"),
        ("PGPASSWORD", "cofla"),
        ("PGDATABASE", "cofla"),
    ]);
    assert!(
        server.startup_line.contains("storage postgres"),
        "line: {}",
        server.startup_line
    );

    let response = server.health_response();
    assert!(response.starts_with("HTTP/1.1 200 OK"), "got: {response}");
    assert!(response.contains("\"storage\":\"postgres\""));

    server.signal(libc::SIGTERM);
    let status = server.wait_within_fail_safe();
    assert!(
        status.success(),
        "exit: {status:?}, stderr: {}",
        server.drain_stderr()
    );
}

fn parse_host_port(url: &str) -> (String, String) {
    let after_auth = url
        .rsplit('@')
        .next()
        .expect("url should contain credentials");
    let authority = after_auth
        .split('/')
        .next()
        .expect("url should contain a path");
    let mut parts = authority.split(':');
    let host = parts.next().expect("host part").to_string();
    let port = parts.next().expect("port part").to_string();
    (host, port)
}
