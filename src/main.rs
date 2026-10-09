mod configuration;

fn main() -> miette::Result<()> {
    miette::set_panic_hook();

    let configuration = configuration::Configuration::new();
    println!("{:?}", configuration);
    Ok(())
}
