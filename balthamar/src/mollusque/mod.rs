pub mod cephalo;
pub mod pode;

// pub trait Mollusque {
//     // Can't run with tentacles...
//     fn swim(&mut self) -> io::Result<()>;
// }

pub enum CephalopodeType {
    Cephalo,
    Pode,
}
