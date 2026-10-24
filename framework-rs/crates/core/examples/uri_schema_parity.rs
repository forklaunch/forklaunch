use std::io::{self, BufRead};

fn main() {
    let input = io::stdin();
    let mut lines = input.lock().lines();
    let schema = serde_json::from_str(&lines.next().unwrap().unwrap()).unwrap();
    let validate = forklaunch_core::contract::schema_validation::compile(&schema).unwrap();
    for line in lines {
        let value = serde_json::from_str(&line.unwrap()).unwrap();
        println!("{}", validate(&value).is_ok());
    }
}
