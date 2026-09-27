//! Простой HTTP-сервер на голом TCP для раздачи ассетов уровня.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use anyhow::Result;

use crate::paths::AppPaths;

pub struct HttpServerHandle {
    pub port: u16,
    shutdown: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl HttpServerHandle {
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(j) = self.join.take() {
            let _ = j.join();
        }
    }
}

impl Drop for HttpServerHandle {
    fn drop(&mut self) { self.shutdown(); }
}

pub fn start_http_server(
    paths: AppPaths,
    level_number: u32,
    port: u16,
    lobby_name: String,
) -> Result<HttpServerHandle> {
    let listener = TcpListener::bind(("0.0.0.0", port))?;
    let shutdown = Arc::new(AtomicBool::new(false));
    let shutdown2 = shutdown.clone();
    let level_xml_path = paths.level_xml(level_number);
    let cache_dir = paths.cache_files_dir.clone();

    let join = thread::spawn(move || {
        for stream in listener.incoming() {
            if shutdown2.load(Ordering::SeqCst) { break; }
            match stream {
                Ok(s) => {
                    let xml = level_xml_path.clone();
                    let cache = cache_dir.clone();
                    let lobby = lobby_name.clone();
                    let ln = level_number;
                    thread::spawn(move || {
                        let _ = handle_client(s, &xml, &cache, &lobby, ln);
                    });
                }
                Err(_) => break,
            }
        }
    });

    Ok(HttpServerHandle { port, shutdown, join: Some(join) })
}

fn handle_client(
    mut s: TcpStream,
    level_xml: &PathBuf,
    cache_dir: &PathBuf,
    lobby_name: &str,
    level_number: u32,
) -> Result<()> {
    let peer = s.peer_addr().ok();
    let mut reader = BufReader::new(s.try_clone()?);
    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    let request_line = request_line.trim().to_string();

    loop {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        if line == "\r\n" || line == "\n" || line.is_empty() { break; }
    }

    let parts: Vec<&str> = request_line.split_whitespace().collect();
    if parts.len() < 2 { return write_404(&mut s); }
    let method = parts[0];
    let raw_path = parts[1];

    if method != "GET" { return write_405(&mut s); }

    let path = raw_path.split('?').next().unwrap_or(raw_path);
    let path = url_decode(path);

    println!("[http] {} {} (peer={:?})", method, path, peer);

    match path.as_str() {
        "/level.xml" => {
            let data = std::fs::read(level_xml)?;
            write_response(&mut s, 200, "application/xml", &data)
        }
        "/manifest.json" => {
            let m = serde_json::json!({
                "lobby_name": lobby_name,
                "level_number": level_number,
                "version": env!("CARGO_PKG_VERSION"),
            });
            let data = serde_json::to_vec_pretty(&m)?;
            write_response(&mut s, 200, "application/json", &data)
        }
        "/index.json" => {
            // Список файлов в cache-files/ — нужен клиенту, чтобы знать, что качать.
            let mut entries: Vec<String> = Vec::new();
            let mut walk = |dir: &PathBuf, prefix: &str| -> Result<()> {
                if !dir.is_dir() { return Ok(()); }
                for e in std::fs::read_dir(dir)? {
                    let e = e?;
                    let p = e.path();
                    let name = e.file_name().to_string_lossy().to_string();
                    if p.is_dir() {
                        walk(&p, &format!("{prefix}{name}/"))?;
                    } else {
                        entries.push(format!("{prefix}{name}"));
                    }
                }
                Ok(())
            };
            walk(cache_dir, "")?;
            entries.sort();
            let data = serde_json::to_vec_pretty(&serde_json::json!({ "files": entries }))?;
            write_response(&mut s, 200, "application/json", &data)
        }
        p if p.starts_with("/assets/") => {
            let rel = p.trim_start_matches("/assets/");
            serve_asset(&mut s, cache_dir, rel)
        }
        _ => write_404(&mut s),
    }
}

fn serve_asset(s: &mut TcpStream, cache_dir: &PathBuf, rel: &str) -> Result<()> {
    if rel.contains("..") { return write_404(s); }

    let full = cache_dir.join(rel);
    if !full.is_file() { return write_404(s); }
    let canon_cache = std::fs::canonicalize(cache_dir).ok();
    let canon_full = std::fs::canonicalize(&full).ok();
    if let (Some(cache), Some(f)) = (canon_cache, canon_full) {
        if !f.starts_with(&cache) { return write_404(s); }
    }

    let data = std::fs::read(&full)?;
    let mime = guess_mime(rel);
    write_response(s, 200, mime, &data)
}

fn guess_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") { "image/png" }
    else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg" }
    else if lower.ends_with(".gltf") { "model/gltf+json" }
    else if lower.ends_with(".glb") { "model/gltf-binary" }
    else if lower.ends_with(".bin") { "application/octet-stream" }
    else if lower.ends_with(".xml") { "application/xml" }
    else if lower.ends_with(".json") { "application/json" }
    else { "application/octet-stream" }
}

fn write_response(s: &mut TcpStream, code: u16, mime: &str, body: &[u8]) -> Result<()> {
    let status = match code {
        200 => "OK",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "OK",
    };
    let header = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        code, status, mime, body.len()
    );
    s.write_all(header.as_bytes())?;
    s.write_all(body)?;
    s.flush()?;
    Ok(())
}

fn write_404(s: &mut TcpStream) -> Result<()> {
    write_response(s, 404, "text/plain; charset=utf-8", b"404 Not Found")
}
fn write_405(s: &mut TcpStream) -> Result<()> {
    write_response(s, 405, "text/plain; charset=utf-8", b"405 Method Not Allowed")
}

fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                if let (Some(h), Some(l)) = (hex(bytes[i+1]), hex(bytes[i+2])) {
                    out.push((h << 4) | l);
                    i += 3;
                    continue;
                }
                out.push(b'%'); i += 1;
            }
            b'+' => { out.push(b' '); i += 1; }
            b => { out.push(b); i += 1; }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

pub fn download_file(base_url: &str, rel_path: &str, out_dir: &PathBuf) -> Result<PathBuf> {
    let url = format!("{}/assets/{}", base_url.trim_end_matches('/'), rel_path);
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| anyhow::anyhow!("http download error: {e}"))?;
    let mut bytes = Vec::new();
    use std::io::Read;
    resp.into_reader().read_to_end(&mut bytes)?;
    let full = out_dir.join(rel_path);
    if let Some(p) = full.parent() { std::fs::create_dir_all(p)?; }
    std::fs::write(&full, &bytes)?;
    Ok(full)
}

/// Скачать список файлов хоста.
pub fn fetch_index(base_url: &str) -> Result<Vec<String>> {
    let url = format!("{}/index.json", base_url.trim_end_matches('/'));
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(30))
        .call()
        .map_err(|e| anyhow::anyhow!("http index error: {e}"))?;
    let v: serde_json::Value = resp.into_json()?;
    let arr = v.get("files").and_then(|x| x.as_array()).cloned().unwrap_or_default();
    Ok(arr.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect())
}

pub fn fetch_manifest(base_url: &str) -> Result<serde_json::Value> {
    let url = format!("{}/manifest.json", base_url.trim_end_matches('/'));
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| anyhow::anyhow!("http manifest error: {e}"))?;
    let v: serde_json::Value = resp.into_json()?;
    Ok(v)
}

pub fn download_level_xml(base_url: &str, paths: &AppPaths, level_number: u32) -> Result<PathBuf> {
    let url = format!("{}/level.xml", base_url.trim_end_matches('/'));
    let resp = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| anyhow::anyhow!("http level.xml error: {e}"))?;
    let xml = resp.into_string()?;
    let target = paths.level_xml(level_number);
    if let Some(p) = target.parent() { std::fs::create_dir_all(p)?; }
    std::fs::write(&target, xml.as_bytes())?;
    Ok(target)
}

pub fn probe_host(base_url: &str) -> bool {
    let url = format!("{}/manifest.json", base_url.trim_end_matches('/'));
    ureq::get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .call()
        .is_ok()
}

pub fn lan_ip() -> Option<std::net::IpAddr> {
    let socket = std::net::UdpSocket::bind(("0.0.0.0", 0)).ok()?;
    socket.connect(("8.8.8.8", 80)).ok()?;
    socket.local_addr().ok().map(|a| a.ip())
}

pub fn base_url_for_self(port: u16) -> String {
    match lan_ip() {
        Some(ip) => format!("http://{}:{}", ip, port),
        None => format!("http://127.0.0.1:{}", port),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_decode_basic() {
        assert_eq!(url_decode("/a%20b"), "/a b");
        assert_eq!(url_decode("/a+b"), "/a b");
    }

    #[test]
    fn guess_mime_works() {
        assert_eq!(guess_mime("a.png"), "image/png");
        assert_eq!(guess_mime("a.JPG"), "image/jpeg");
        assert_eq!(guess_mime("a.gltf"), "model/gltf+json");
    }

    #[test]
    fn base_url_for_self_returns_something() {
        let s = base_url_for_self(27016);
        assert!(s.starts_with("http://"));
    }
}
