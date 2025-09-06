use plugin_annotations::{plugin_aggregates, plugin_impl};
use plugin_interface::Greeter;

#[plugin_aggregates(Greeter)]

pub struct MyGreeter;

impl Default for MyGreeter {
    fn default() -> Self {
        MyGreeter
    }
}

#[plugin_impl(Greeter)]
impl Greeter for MyGreeter {
    fn name(&self) -> &str {
        "MyGreeter"
    }
    fn greet(&self, target: &str) {
        println!("Hello, {}! from MyGreeter", target);
    }
}
