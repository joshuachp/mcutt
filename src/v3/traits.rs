use super::packets::publish::{PubRec, PublishOwned};

trait Sender {
    fn store(msg: SenderMsg);

    fn send(msg: SenderMsg);

    fn discard(msg: SenderMsg);
}

enum SenderMsg {
    Publish(PublishOwned),
    Pubrec(PubRec),
}

trait SlabStore<T> {
    fn get_mut(idx: u16) -> Option<T>;
}
