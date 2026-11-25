use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Task {
    pub id: i32,
    pub title: String,
    pub done: bool,
}
