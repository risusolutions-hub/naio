//! Agent identity listing.

use crate::error::{SshError, SshResult};
use crate::key::fingerprint_public;
use crate::runtime::block_on;
use russh::keys::agent::client::AgentClient;
use russh::keys::PublicKey;

/// One identity reported by the agent.
#[derive(Debug, Clone)]
pub struct AgentIdentity {
    pub fingerprint: String,
    pub algorithm: String,
    pub comment: String,
}

/// List identities known to the local SSH agent (Pageant / pipe / SSH_AUTH_SOCK).
pub fn agent_identities() -> SshResult<Vec<AgentIdentity>> {
    block_on(agent_identities_async())
}

async fn agent_identities_async() -> SshResult<Vec<AgentIdentity>> {
    let keys = with_agent(|mut client| {
        Box::pin(async move {
            client
                .request_identities()
                .await
                .map_err(|e| SshError::Agent(e.to_string()))
        })
    })
    .await?;
    Ok(keys
        .into_iter()
        .map(|k: PublicKey| AgentIdentity {
            fingerprint: fingerprint_public(&k),
            algorithm: k.algorithm().to_string(),
            comment: k.comment().to_string(),
        })
        .collect())
}

/// Run a closure with a connected dynamic agent client.
pub(crate) async fn connect_agent(
) -> SshResult<AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>> {
    #[cfg(unix)]
    {
        let c = AgentClient::connect_env()
            .await
            .map_err(|e| SshError::Agent(e.to_string()))?;
        return Ok(c.dynamic());
    }
    #[cfg(windows)]
    {
        let pipe = r"\\.\pipe\openssh-ssh-agent";
        if let Ok(c) = AgentClient::connect_named_pipe(pipe).await {
            // Probe
            let mut c = c.dynamic();
            match c.request_identities().await {
                Ok(_) => return Ok(AgentClient::connect_named_pipe(pipe).await?.dynamic()),
                Err(e) => {
                    let _ = e;
                }
            }
        }
        let mut p = AgentClient::connect_pageant().await.dynamic();
        match p.request_identities().await {
            Ok(_) => Ok(AgentClient::connect_pageant().await.dynamic()),
            Err(e) => Err(SshError::Agent(format!("no SSH agent: {e}"))),
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(SshError::Agent(
            "SSH agent unsupported on this platform".into(),
        ))
    }
}

async fn with_agent<T, F, Fut>(f: F) -> SshResult<T>
where
    F: FnOnce(AgentClient<Box<dyn russh::keys::agent::client::AgentStream + Send + Unpin>>) -> Fut,
    Fut: std::future::Future<Output = SshResult<T>>,
{
    let client = connect_agent().await?;
    f(client).await
}
