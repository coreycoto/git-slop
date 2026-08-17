#[cfg(test)]
mod tests {
    use super::*;

    include!("tests/contracts.rs");
    include!("tests/execution.rs");
    include!("tests/finalization.rs");
    include!("tests/receipts.rs");
}
