#[cfg(feature = "ws")]
mod ws;

#[cfg(feature = "ws")]
fn main() {
    ws::main()
}

#[cfg(not(feature = "ws"))]
fn main() {
    eprintln!("Must build with ws feature");
    unimplemented!()
}
