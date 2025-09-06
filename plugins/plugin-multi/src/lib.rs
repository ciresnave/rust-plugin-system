use plugin_annotations::{plugin_aggregates, plugin_impl};
use plugin_interface::Greeter;

#[plugin_aggregates(Greeter)]
#[derive(Default)]
struct GreeterOne;

#[plugin_impl(Greeter)]
impl Greeter for GreeterOne {
    fn name(&self) -> &str {
        "GreeterOne"
    }
    fn greet(&self, target: &str) {
        println!("Hello, {} from GreeterOne", target);
    }
}

#[derive(Default)]
struct GreeterTwo;

#[plugin_impl(Greeter)]
impl Greeter for GreeterTwo {
    fn name(&self) -> &str {
        "GreeterTwo"
    }
    fn greet(&self, target: &str) {
        println!("Hello, {} from GreeterTwo", target);
    }
}
