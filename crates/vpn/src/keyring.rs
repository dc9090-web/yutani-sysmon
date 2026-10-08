//! The qBittorrent Web UI password, in the Secret Service keyring only.

use zeroize::Zeroizing;

const ATTR: (&str, &str) = ("application", "cosmic-applet-vpn/qbit-webui");
const LABEL: &str = "qBittorrent Web UI password (COSMIC VPN applet)";

pub async fn load() -> Zeroizing<String> {
    let r: oo7::Result<Option<Zeroizing<String>>> = async {
        let k = oo7::Keyring::new().await?;
        k.unlock().await?;
        let Some(item) = k.search_items(&[ATTR]).await?.into_iter().next() else { return Ok(None) };
        if item.is_locked().await? {
            item.unlock().await?;
        }
        let secret = item.secret().await?;
        Ok(std::str::from_utf8(secret.as_bytes()).ok().map(|s| Zeroizing::new(s.to_owned())))
    }
    .await;
    match r {
        Ok(p) => p.unwrap_or_default(),
        Err(e) => {
            tracing::debug!("keyring: {e}");
            Zeroizing::default()
        }
    }
}

/// Saves the password (an empty one removes it). `true` if it was stored.
pub async fn store(pass: Zeroizing<String>) -> bool {
    let r: oo7::Result<()> = async {
        let k = oo7::Keyring::new().await?;
        k.unlock().await?;
        if pass.is_empty() { k.delete(&[ATTR]).await } else { k.create_item(LABEL, &[ATTR], pass.as_str(), true).await }
    }
    .await;
    if let Err(e) = &r {
        tracing::warn!("saving the Web UI password: {e}");
    }
    r.is_ok()
}
