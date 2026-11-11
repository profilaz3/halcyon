//! Downloads against a local server on 127.0.0.1; no network.
use super::*;
use std::io::{Read, Write};

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "halcyon-model-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Serves `body` to each connection, `chunk` bytes at a time with `pause`
/// between them; `status` is the HTTP status line's code.
fn serve(
    body: Vec<u8>,
    status: u16,
    chunk: usize,
    pause: Duration,
) -> (String, std::thread::JoinHandle<()>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
    let handle = std::thread::spawn(move || {
        listener.set_nonblocking(false).unwrap();
        // Serve up to two requests (a retry), then stop.
        for _ in 0..2 {
            let Ok((mut stream, _)) = listener.accept() else {
                return;
            };
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request);
            let head = format!(
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            if stream.write_all(head.as_bytes()).is_err() {
                continue;
            }
            for part in body.chunks(chunk.max(1)) {
                if stream.write_all(part).is_err() {
                    break;
                }
                std::thread::sleep(pause);
            }
        }
    });
    (url, handle)
}

fn source(url: &str, body: &[u8]) -> Source {
    Source {
        url: url.into(),
        size: body.len() as u64,
        sha256: hex(&Sha256::digest(body)),
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
fn own_wallpaper_model_pin_is_the_reviewed_file() {
    let pinned = pinned();
    assert_eq!(
        pinned.url,
        "https://huggingface.co/onnx-community/depth-anything-v2-small/resolve/4472b7362082ad9968fee890ca0f1e5aca36b93d/onnx/model.onnx"
    );
    assert_eq!(pinned.size, 99_060_839);
    assert_eq!(
        pinned.sha256,
        "afb6a5c28f3b6bf1618c6e43f02073ef9dfdc70e937502d51603e57b0a1df10c"
    );
    assert!(
        path().is_none_or(|p| p.ends_with("halcyon/models/depth-anything-v2-small-4472b73.onnx"))
    );
}

#[test]
fn own_wallpaper_model_downloads_checks_and_renames() {
    let body: Vec<u8> = (0..200_000u32).map(|i| (i * 7 % 251) as u8).collect();
    let (url, _server) = serve(body.clone(), 200, 16 * 1024, Duration::ZERO);
    let dir = temp("ok");
    let target = dir.join("model.onnx");
    let (_tx, cancel) = watch::channel(false);
    let mut seen = Vec::new();
    runtime()
        .block_on(download(&source(&url, &body), &target, cancel, |n| {
            seen.push(n)
        }))
        .unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), body);
    assert!(!target.with_extension("part").exists());
    assert_eq!(seen.last(), Some(&(body.len() as u64)));
    assert!(seen.windows(2).all(|w| w[0] < w[1]), "progress only grows");
    assert!(installed(&target, &source(&url, &body)));
    assert_eq!(read_verified(&target, &source(&url, &body)).unwrap(), body);
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn own_wallpaper_model_rejects_bad_hash_and_wrong_size() {
    let body: Vec<u8> = vec![3; 50_000];
    let dir = temp("bad");
    let target = dir.join("model.onnx");
    let (_tx, cancel) = watch::channel(false);
    // Right size, wrong content.
    let (url, _server) = serve(body.clone(), 200, 8192, Duration::ZERO);
    let mut wrong = source(&url, &body);
    wrong.sha256 = "0".repeat(64);
    assert_eq!(
        runtime().block_on(download(&wrong, &target, cancel.clone(), |_| {})),
        Err(Error::Hash)
    );
    assert!(!target.exists() && !target.with_extension("part").exists());
    // A server sending a different length.
    let (url, _server) = serve(body.clone(), 200, 8192, Duration::ZERO);
    let mut short = source(&url, &body);
    short.size += 1;
    assert_eq!(
        runtime().block_on(download(&short, &target, cancel.clone(), |_| {})),
        Err(Error::Size)
    );
    assert!(!target.exists() && !target.with_extension("part").exists());
    // Not the file at all.
    let (url, _server) = serve(b"gone".to_vec(), 404, 8192, Duration::ZERO);
    assert_eq!(
        runtime().block_on(download(
            &source(&url, &body),
            &target,
            cancel.clone(),
            |_| {}
        )),
        Err(Error::Status(404))
    );
    // A file on disk is checked again before use.
    std::fs::write(&target, vec![4; 50_000]).unwrap();
    assert!(installed(&target, &source(&url, &body)));
    assert_eq!(
        read_verified(&target, &source(&url, &body)),
        Err(Error::Hash)
    );
    std::fs::write(&target, vec![3; 49_999]).unwrap();
    assert_eq!(
        read_verified(&target, &source(&url, &body)),
        Err(Error::Size)
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn own_wallpaper_model_cancel_removes_the_part_and_retry_succeeds() {
    let body: Vec<u8> = (0..400_000u32).map(|i| (i % 253) as u8).collect();
    // Slow enough that cancelling lands mid-stream; the second request is the retry.
    let (url, _server) = serve(body.clone(), 200, 4096, Duration::from_millis(5));
    let dir = temp("cancel");
    let target = dir.join("model.onnx");
    let src = source(&url, &body);
    let rt = runtime();
    let (tx, cancel) = watch::channel(false);
    let started = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = started.clone();
    let result = rt.block_on(async {
        let download = download(&src, &target, cancel, move |n| {
            if n > 20_000 {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            }
        });
        let stop = async {
            while !started.load(std::sync::atomic::Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            tx.send(true).unwrap();
            std::future::pending::<()>().await
        };
        tokio::select! { r = download => r, _ = stop => unreachable!() }
    });
    assert_eq!(result, Err(Error::Cancelled));
    assert!(!target.exists() && !target.with_extension("part").exists());
    // Try again: the same server's second answer, uncancelled.
    let (_tx, cancel) = watch::channel(false);
    rt.block_on(download(&src, &target, cancel, |_| {}))
        .unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), body);
    // Remove takes the model and the analyses with it.
    let scenes = dir.join("depth");
    std::fs::create_dir_all(scenes.join("x")).unwrap();
    remove(&target, Some(&scenes));
    assert!(!target.exists() && !scenes.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn own_wallpaper_model_unreachable_is_a_network_error() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/model.onnx", listener.local_addr().unwrap());
    drop(listener);
    let dir = temp("down");
    let (_tx, cancel) = watch::channel(false);
    assert_eq!(
        runtime().block_on(download(
            &source(&url, b"x"),
            &dir.join("m.onnx"),
            cancel,
            |_| {}
        )),
        Err(Error::Network)
    );
    std::fs::remove_dir_all(dir).unwrap();
}
