mod configuration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let configuration = configuration::Configuration::new();
    println!("{:?}", configuration);
    Ok(())
}
