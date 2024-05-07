use bytes::{Buf, Bytes, BytesMut};

use super::error::GrpcSshError;
use tokio_util::codec::Decoder;

type MessageTypeId = u8;
// Ref: https://datatracker.ietf.org/doc/html/draft-miller-ssh-agent-04#section-5.1
const SSH_AGENT_FAILURE: MessageTypeId = 5;
const SSH_AGENT_SUCCESS: MessageTypeId = 6;
pub const SSH_AGENTC_REQUEST_IDENTITIES: MessageTypeId = 11;
pub const SSH_AGENTC_SIGN_RESPONSE: MessageTypeId = 13;
pub const SSH_AGENTC_EXTENSION: MessageTypeId = 27;

pub const MAX_MESSAGE_SIZE: u32 = 1024 * 1024;

#[derive(Debug, Copy, Clone)]
enum SshAgentPacketDecoderState {
    WaitingHeader,
    WaitingPayload(u8, u32), // StreamType, Length
}

#[derive(Debug)]
pub(crate) struct SshAgentPacketDecoder {
    state: SshAgentPacketDecoderState,
}

impl SshAgentPacketDecoder {
    #[inline]
    pub(crate) fn new() -> SshAgentPacketDecoder {
        Self {
            state: SshAgentPacketDecoderState::WaitingHeader,
        }
    }
}

impl Decoder for SshAgentPacketDecoder {
    type Item = Bytes;
    type Error = GrpcSshError;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        loop {
            match self.state {
                SshAgentPacketDecoderState::WaitingHeader => {
                    log::trace!("waitingheader: {}", src.len());
                    if src.len() < 5 {
                        return Ok(None);
                    }

                    let mut header = &src[0..5];
                    let len = header.get_u32(); // big-endian, length minus header
                    let message_type = header.get_u8();
                    self.state = SshAgentPacketDecoderState::WaitingPayload(message_type, len);
                }
                SshAgentPacketDecoderState::WaitingPayload(message_type, length) => {
                    log::trace!("waitingpayload: {}, {}", src.len(), length);
                    if u32::try_from(src.len())? < length {
                        log::trace!("waiting: not enough data: {}, {}", src.len(), length);
                        return Ok(None);
                    } else {
                        log::trace!("SshAgentPacketDecoder: Reading payload");
                        let message = src.split_to(src.len()).freeze();

                        match message_type {
                            SSH_AGENTC_REQUEST_IDENTITIES | SSH_AGENTC_SIGN_RESPONSE => {
                                self.state = SshAgentPacketDecoderState::WaitingHeader;
                                return Ok(Some(message));
                            }

                            SSH_AGENTC_EXTENSION => {
                                // no-op the ssh agent extension message type

                                log::warn!("sshforward extension not supported");
                                src.advance(src.len());
                                self.state = SshAgentPacketDecoderState::WaitingHeader;

                                return Ok(Some(Bytes::from_static(b"\0\0\0\x01\x06")));
                            }

                            e => {
                                self.state = SshAgentPacketDecoderState::WaitingHeader;
                                return Err(GrpcSshError::InvalidMessageType(e));
                            }
                        }
                    }
                }
            }
        }
    }
}
