export function ErrorBanner({ message }: { message: string }) {
  return (
    <div role="alert" className="banner">
      {message}
    </div>
  );
}
