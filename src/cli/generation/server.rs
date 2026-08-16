fn open_report_url(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = std::process::Command::new("cmd");
        command.args(["/C", "start", ""]);
        command
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    let status = command.arg(url).status()?;
    if !status.success() {
        bail!("failed to open the report URL with the system browser");
    }
    Ok(())
}

fn serve_report(html: &str, seconds: u64, open: bool) -> Result<()> {
    let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))?;
    listener.set_nonblocking(true)?;
    let address = listener.local_addr()?;
    let url = format!("http://{address}/");
    println!("Serving the local report at {url}");
    println!("Loopback only; press Ctrl-C to stop, or wait {seconds} second(s).");
    if open {
        open_report_url(&url)?;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    while std::time::Instant::now() < deadline {
        match listener.accept() {
            Ok((mut stream, _)) => {
                // A client that connects and never finishes a request must not keep the
                // temporary server alive past its advertised lifetime.
                let remaining = deadline.saturating_duration_since(std::time::Instant::now());
                let connection_timeout = remaining
                    .min(std::time::Duration::from_secs(2))
                    .max(std::time::Duration::from_millis(100));
                stream.set_read_timeout(Some(connection_timeout))?;
                stream.set_write_timeout(Some(connection_timeout))?;
                let mut request = [0_u8; 2048];
                let read = stream.read(&mut request).unwrap_or_default();
                let request = String::from_utf8_lossy(&request[..read]);
                let body = if request.starts_with("GET / ") { html } else { "Not found" };
                let status = if request.starts_with("GET / ") {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:\r\nX-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                )?;
                stream.flush()?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            Err(error) => return Err(error.into()),
        }
    }
    println!("Stopped the temporary report server.");
    Ok(())
}
