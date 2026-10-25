//! Explicitly consented, bounded downloads from Modrinth's file CDN only.
use super::{Download, MAX_FILE, Result, discovery, verify_replacement};
use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub(super) struct Staged(PathBuf);
impl Staged {
    pub fn path(&self) -> &Path {
        &self.0
    }
}
impl Drop for Staged {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_file(&self.0) {
            crate::cache::warning(&format!("Cannot remove staged mod: {e}"));
        }
    }
}

pub(super) fn stage(workspace: &Path, download: &Download, cancel: &AtomicBool) -> Result<Staged> {
    super::validate_download(download)?;
    discovery::safe_path(workspace)?;
    let staging = workspace.join("staging");
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    discovery::safe_path(&staging)?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let path = staging.join(format!("verified-{}-{stamp}.jar", std::process::id()));
    let file = File::options()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|e| e.to_string())?;
    let staged = Staged(path);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| e.to_string())?;
    runtime.block_on(transfer(download, file, cancel))?;
    verify_replacement(staged.path(), download)?;
    Ok(staged)
}

async fn transfer(download: &Download, file: File, cancel: &AtomicBool) -> Result<()> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(120))
        .user_agent("MinecraftAiDoctor/0.1.0 (user-reviewed Modrinth repair copy)")
        .build()
        .map_err(|e| e.to_string())?;
    receive(&client, &download.url, download.size, file, cancel).await
}

async fn receive(
    client: &reqwest::Client,
    url: &str,
    expected_size: u64,
    mut file: File,
    cancel: &AtomicBool,
) -> Result<()> {
    let operation = async {
        for attempt in 0..3 {
            if cancel.load(Ordering::Relaxed) {
                return Err("Download cancelled".into());
            }
            let mut response = client
                .get(url)
                .send()
                .await
                .map_err(|_| "Modrinth mod download failed or timed out".to_string())?;
            let status = response.status();
            if (status.as_u16() == 429 || status.is_server_error()) && attempt < 2 {
                let seconds = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(1 << attempt)
                    .clamp(1, 10);
                tokio::time::sleep(Duration::from_secs(seconds)).await;
                continue;
            }
            if !status.is_success() {
                return Err(format!("Mod download returned HTTP {}", status.as_u16()));
            }
            if response
                .content_length()
                .is_some_and(|n| n != expected_size)
            {
                return Err("Mod response size differs from reviewed metadata".into());
            }
            let mut size = 0_u64;
            while let Some(bytes) = response
                .chunk()
                .await
                .map_err(|_| "Cannot read mod download".to_string())?
            {
                size = size.saturating_add(bytes.len() as u64);
                if size > expected_size || size > MAX_FILE {
                    return Err("Mod download exceeds reviewed size".into());
                }
                file.write_all(&bytes).map_err(|e| e.to_string())?;
            }
            if size != expected_size {
                return Err("Incomplete mod download".into());
            }
            file.sync_all().map_err(|e| e.to_string())?;
            return Ok(());
        }
        Err("Mod download retries exhausted".into())
    };
    let cancellation = async {
        loop {
            if cancel.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(60)).await;
        }
    };
    tokio::select! {result=operation=>result,_=cancellation=>Err("Download cancelled".into())}
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Read, net::TcpListener};
    #[tokio::test(flavor = "current_thread")]
    async fn synthetic_transfer_checks_size_retries_and_cancel() {
        let temp = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            for response in [
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\nRetry-After: 1\r\nConnection: close\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 7\r\nConnection: close\r\n\r\nfixture",
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut request = [0; 2048];
                assert!(stream.read(&mut request).unwrap() > 0);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        let client = reqwest::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(5))
            .build()
            .unwrap();
        let path = temp.path().join("fixture.jar");
        receive(
            &client,
            &url,
            7,
            File::create(&path).unwrap(),
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        server.join().unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"fixture");
        assert!(
            receive(
                &client,
                "http://127.0.0.1:1",
                7,
                File::create(temp.path().join("cancel.jar")).unwrap(),
                &AtomicBool::new(true)
            )
            .await
            .unwrap_err()
            .contains("cancelled")
        );
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut request = [0; 2048];
            assert!(stream.read(&mut request).unwrap() > 0);
            stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100\r\nConnection: close\r\n\r\n")
                .unwrap();
        });
        assert!(
            receive(
                &client,
                &url,
                7,
                File::create(temp.path().join("oversized.jar")).unwrap(),
                &AtomicBool::new(false)
            )
            .await
            .unwrap_err()
            .contains("size differs")
        );
        server.join().unwrap();
    }
}
