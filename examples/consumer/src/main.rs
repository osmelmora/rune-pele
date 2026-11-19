use rune_pele::rune::{self, AppSpec};
use rune_pele_consumer::{init, view};

fn main() {
    rune::run(AppSpec::new("rune-pele consumer", (720.0, 480.0), init, view));
}
