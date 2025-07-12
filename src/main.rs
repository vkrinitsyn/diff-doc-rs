/**
usage examples

*/

#[tokio::main(flavor = "multi_thread", worker_threads = 10)]
async fn main() {
    println!("ok");
}


mod tests {
    #[tokio::test]
    async fn test_compile() {
        assert!(true);
    }
}