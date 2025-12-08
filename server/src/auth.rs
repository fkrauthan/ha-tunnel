use common::now_as_secs;
use common::tunnel::generate_auth_signature;

pub fn verify_auth_signature(
    client_id: &str,
    timestamp: u64,
    signature: &str,
    secret: &str,
) -> bool {
    // Check timestamp is within 1 minute
    let now = now_as_secs();
    if now.abs_diff(timestamp) > 60 {
        return false;
    }

    let expected = generate_auth_signature(client_id, timestamp, secret);
    expected.len() == signature.len()
        && expected.bytes().zip(signature.bytes()).all(|(a, b)| a == b)
}
