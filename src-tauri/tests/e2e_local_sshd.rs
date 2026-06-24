//! End-to-end SFTP+SSH integration test against a real OpenSSH server.
//!
//! Skipped automatically if the env var SKYHOOK_E2E_SSHD is not set, so CI on
//! Windows/macOS without sshd doesn't fail. Run on Linux with:
//!
//!   SKYHOOK_E2E_SSHD=127.0.0.1:2222 \
//!   SKYHOOK_E2E_KEY=/tmp/skyhook-e2e/etc/test_user \
//!   SKYHOOK_E2E_USER=root \
//!     cargo test --release --test e2e_local_sshd -- --nocapture
//!
//! Exercises the same russh/russh-sftp code paths the SessionActor uses:
//! TCP connect → SSH auth (pubkey) → SFTP open → list /, mkdir, write, read,
//! roundtrip-compare, remove, rmdir → shell exec → graceful disconnect.

use russh::client;
use russh_keys::decode_secret_key;
use russh_sftp::client::SftpSession;
use std::sync::Arc;
use std::time::Duration;

struct H;

#[async_trait::async_trait]
impl client::Handler for H {
    type Error = russh::Error;
    async fn check_server_key(
        &mut self,
        _key: &russh_keys::key::PublicKey,
    ) -> Result<bool, Self::Error> {
        Ok(true) // TOFU is tested elsewhere; here we just need a session
    }
}

fn env_or_skip(k: &str) -> Option<String> {
    std::env::var(k).ok()
}

#[tokio::test]
async fn e2e_sftp_and_shell_roundtrip() {
    let Some(addr) = env_or_skip("SKYHOOK_E2E_SSHD") else {
        eprintln!("SKYHOOK_E2E_SSHD not set; skipping");
        return;
    };
    let key_path = env_or_skip("SKYHOOK_E2E_KEY").expect("SKYHOOK_E2E_KEY required");
    let user = env_or_skip("SKYHOOK_E2E_USER").unwrap_or_else(|| "root".into());

    let key_pem = std::fs::read_to_string(&key_path).expect("read key");
    let kp = decode_secret_key(&key_pem, None).expect("parse key");

    let (host, port) = {
        let (h, p) = addr.rsplit_once(':').expect("host:port");
        (h.to_string(), p.parse::<u16>().expect("port"))
    };

    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(30)),
        ..Default::default()
    });

    // --- Connect + auth ---
    let mut handle = client::connect(config, (host.as_str(), port), H)
        .await
        .expect("connect");
    let ok = handle
        .authenticate_publickey(user, Arc::new(kp))
        .await
        .expect("auth");
    assert!(ok, "publickey auth must succeed");

    // --- Open SFTP subsystem ---
    let channel = handle.channel_open_session().await.expect("channel");
    channel
        .request_subsystem(true, "sftp")
        .await
        .expect("subsystem sftp");
    let sftp = SftpSession::new(channel.into_stream())
        .await
        .expect("sftp session");

    // --- List root ---
    let entries = sftp.read_dir("/").await.expect("read_dir /");
    let names: Vec<String> = entries.map(|e| e.file_name()).collect();
    assert!(names.iter().any(|n| n == "etc" || n == "tmp"), "/ should contain etc or tmp, got {names:?}");

    // --- mkdir + write + read + verify + remove ---
    let test_dir = "/tmp/skyhook-e2e-rust";
    let _ = sftp.remove_dir(test_dir).await; // pre-clean
    sftp.create_dir(test_dir).await.expect("mkdir");
    let test_file = format!("{test_dir}/hello.txt");
    let payload = b"hello from skyhook e2e roundtrip\n";
    {
        use russh_sftp::client::fs::File;
        let mut f = sftp
            .create(&test_file)
            .await
            .expect("create");
        use tokio::io::AsyncWriteExt;
        f.write_all(payload).await.expect("write");
        f.flush().await.expect("flush");
        let _: File = f; // keep type explicit
    }
    let got = sftp.read(&test_file).await.expect("read back");
    assert_eq!(got, payload, "roundtrip bytes must match");

    let st = sftp.metadata(&test_file).await.expect("stat");
    assert_eq!(st.size.unwrap_or(0), payload.len() as u64);

    sftp.remove_file(&test_file).await.expect("rm");
    sftp.remove_dir(test_dir).await.expect("rmdir");

    // --- Shell exec via a *separate* channel (Skyhook's shell.rs pattern) ---
    let exec_chan = handle.channel_open_session().await.expect("exec channel");
    exec_chan.exec(true, "echo skyhook-e2e-ok").await.expect("exec");
    let mut out = Vec::new();
    let mut exit: Option<u32> = None;
    let mut chan = exec_chan;
    while let Some(msg) = chan.wait().await {
        match msg {
            russh::ChannelMsg::Data { ref data } => out.extend_from_slice(data),
            russh::ChannelMsg::ExtendedData { ref data, ext: _ } => out.extend_from_slice(data),
            russh::ChannelMsg::ExitStatus { exit_status } => {
                exit = Some(exit_status);
                // Don't break — server typically sends ExitStatus then Eof/Close.
                // Breaking here races and is the bug this test originally caught.
            }
            russh::ChannelMsg::Close => break,
            _ => {}
        }
    }
    assert_eq!(exit, Some(0), "exec must exit 0");
    let stdout = String::from_utf8_lossy(&out);
    assert!(stdout.contains("skyhook-e2e-ok"), "stdout was: {stdout}");

    // --- Clean disconnect ---
    handle
        .disconnect(russh::Disconnect::ByApplication, "bye", "en")
        .await
        .expect("disconnect");
}
