/* src/quic/auth.rs */

use quinn::{Connection, RecvStream, SendStream};
use tokio::time::{timeout, Duration};

pub async fn authenticate(
    connection: &Connection,
    expected_token: &str,
) -> Result<(SendStream, RecvStream), ()> {
    let (mut send, mut recv) = match connection.accept_bi().await {
        Ok(stream) => stream,
        Err(_) => return Err(()),
    };

    let token_data = match timeout(Duration::from_secs(10), recv.read_to_end(1024)).await {
        Ok(Ok(data)) => data,
        _ => return Err(()),
    };

    let received_token = match String::from_utf8(token_data) {
        Ok(token) => token,
        Err(_) => return Err(()),
    };

    if received_token != expected_token {
        let _ = send.write_all(b"AUTH_FAILED").await;
        let _ = send.finish();
        return Err(());
    }

    if send.write_all(b"AUTH_SUCCESS").await.is_err() || send.finish().is_err() {
        return Err(());
    }

    Ok((send, recv))
}
