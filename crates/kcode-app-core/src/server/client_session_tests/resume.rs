use super::*;
use crate::transport::WriteHalf;
use anyhow::{Result, anyhow};

fn test_writer() -> Result<(Arc<Mutex<WriteHalf>>, crate::transport::Stream)> {
    let (stream_a, stream_b) = crate::transport::stream_pair().map_err(|e| anyhow!(e))?;
    let (_reader, writer_half) = stream_a.into_split();
    Ok((Arc::new(Mutex::new(writer_half)), stream_b))
}

use crate::protocol::SwarmLifecycleStatus;
include!("resume/multiple_live_attach.rs");
include!("resume/busy_existing_attach.rs");
include!("resume/reconnect_takeover_with_history.rs");
include!("resume/attach_without_local_history.rs");
include!("resume/different_client_attach.rs");
include!("resume/live_events_before_history.rs");
include!("resume/same_client_takeover.rs");
