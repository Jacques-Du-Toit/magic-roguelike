//! Headless simulation CLI. It will run AI-vs-AI games from M1; for now it
//! only proves that a binary can link against the engine.

fn main() {
    let info = engine::engine_info();
    println!("{} {}", info.name, info.version);

    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        println!(
            "Simulation is not implemented yet (milestone M1). Ignoring arguments: {}",
            args.join(" ")
        );
    }
}
