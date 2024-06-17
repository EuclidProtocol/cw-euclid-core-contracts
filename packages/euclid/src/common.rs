pub fn generate_id(sender: &str, count: u128) -> String {
    format!("{sender}-{count}")
}
