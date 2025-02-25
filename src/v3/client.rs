use core::marker::PhantomData;
use core::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;

use fnv::FnvHashMap;

use crate::slab::{Access, Entry, Slab};
use crate::v3::packets::connect::ReturnCode;

use super::packets::connect::{ConnAck, Connect};
use super::packets::header::PacketId;
use super::packets::packet::Packet;

struct Client {
    // TODO: state of the packages
    incoming: FnvHashMap<PacketId, ()>,
    // TODO: state of the packages
    outgoing: Slab<Entry<Vec<()>>>,
    // TODO: ping state
    keepalive: bool,
    /// Client is waiting for the CONNACK
    wait_for_connack: bool,
}

struct Keepalive {
    instant: Instant,
    sent: Arc<AtomicBool>,
}

impl Client {
    fn connect(packet: &Connect) -> Result<(), ()> {
        if packet.has_clean_session() {}

        Ok(())
    }

    fn handle_incoming(&mut self, packet: &Packet<'_>) -> Result<(), ()> {
        if self.wait_for_connack && !packet.is_conn_ack() {
            return Err(());
        }

        match packet {
            Packet::ConnAck(conn_ack) => self.handle_connack(conn_ack),
            Packet::Publish(publish) => {}
            Packet::PubAck(pub_ack) => {}
            Packet::PubRec(pub_rec) => {}
            Packet::PubRel(pub_rel) => {}
            Packet::PubComp(pub_comp) => {}
            Packet::SubAck(sub_ack) => {}
            Packet::UnsubAck(unsub_ack) => {}
            Packet::PingResp(ping_resp) => {}
        }
    }

    fn handle_connack(&mut self, conn_ack: &ConnAck) -> Result<(), ()> {
        match conn_ack.return_code() {
            ReturnCode::Accepted => {
                if !conn_ack.session_present() {
                    self.new_session()
                }

                Ok(())
            }
            ReturnCode::ProtocolVersion => todo!(),
            ReturnCode::IdentifierRejected => todo!(),
            ReturnCode::ServerUnavailable => todo!(),
            ReturnCode::BadUsernamePassword => todo!(),
            ReturnCode::NotAuthorized => todo!(),
        }

        todo!()
    }

    /// The Session state in the Client consists of:
    /// - QoS 1 and QoS 2 messages which have been sent to the Server, but have not been completely acknowledged.
    /// - QoS 2 messages which have been received from the Server, but have not been completely acknowledged.
    fn new_session(&self) {
        // Dis
    }
}
