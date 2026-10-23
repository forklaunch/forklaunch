//! Owned `PostgreSQL` wire fixture: forwards startup/auth/query frames, but drops
//! exactly one armed COMMIT acknowledgement after `PostgreSQL` reports success.
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
pub async fn start(host: String, port: u16) -> (u16, Arc<AtomicBool>, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy_port = listener.local_addr().unwrap().port();
    let armed = Arc::new(AtomicBool::new(false));
    let flag = armed.clone();
    let server = tokio::spawn(async move {
        while let Ok((frontend, _)) = listener.accept().await {
            let host = host.clone();
            let armed = flag.clone();
            tokio::spawn(async move {
                let _ = connection(frontend, &host, port, armed).await;
            });
        }
    });
    (proxy_port, armed, server)
}
async fn connection(
    mut frontend: TcpStream,
    host: &str,
    port: u16,
    armed: Arc<AtomicBool>,
) -> std::io::Result<()> {
    let mut backend = TcpStream::connect((host, port)).await?;
    let length = frontend.read_u32().await?;
    if !(8..=65536).contains(&length) {
        return Err(std::io::Error::other("invalid startup length"));
    }
    let mut payload = vec![0; length as usize - 4];
    frontend.read_exact(&mut payload).await?;
    backend.write_u32(length).await?;
    backend.write_all(&payload).await?;
    let (mut fr, mut fw) = frontend.into_split();
    let (mut br, mut bw) = backend.into_split();
    let inbound = async {
        loop {
            let mut header = [0; 5];
            fr.read_exact(&mut header).await?;
            let length = u32::from_be_bytes(header[1..].try_into().unwrap());
            if !(4..=2 * 1024 * 1024).contains(&length) {
                return Err(std::io::Error::other("invalid frontend frame"));
            }
            let mut payload = vec![0; length as usize - 4];
            fr.read_exact(&mut payload).await?;
            bw.write_all(&header).await?;
            bw.write_all(&payload).await?;
        }
        #[allow(unreachable_code)]
        Ok::<(), std::io::Error>(())
    };
    let outbound = async {
        loop {
            let mut header = [0; 5];
            br.read_exact(&mut header).await?;
            let length = u32::from_be_bytes(header[1..].try_into().unwrap());
            if !(4..=2 * 1024 * 1024).contains(&length) {
                return Err(std::io::Error::other("invalid backend frame"));
            }
            let mut payload = vec![0; length as usize - 4];
            br.read_exact(&mut payload).await?;
            if header[0] == b'C' && payload == b"COMMIT\0" && armed.swap(false, Ordering::SeqCst) {
                fw.shutdown().await?;
                return Ok(());
            }
            fw.write_all(&header).await?;
            fw.write_all(&payload).await?;
        }
    };
    tokio::select! {result=inbound=>result,result=outbound=>result}
}
