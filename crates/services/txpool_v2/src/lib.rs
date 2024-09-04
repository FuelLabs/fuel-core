mod service;

pub use service::{
    new_service,
    Service,
    SharedState
};

#[test]
fn it_works() {
    assert_eq!(1, 1);
}