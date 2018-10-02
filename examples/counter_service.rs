extern crate witnet;

use witnet::services::counter;

fn main() {
    let c = counter::start(0);

    println!("Get count: {:?}", counter::get(&c));
    println!("Set count to 10: {:?}", counter::set(&c, 10));
    println!("Get count: {:?}", counter::get(&c));
    println!("Stop counter: {:?}", counter::stop(&c));
}
