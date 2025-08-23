use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // A list of files to search
    #[arg(short, long, num_args = 1.., action = clap::ArgAction::Append)]
    files: Vec<String>,

    // A list of text-passages to search
    #[arg(short, long, num_args = 1.., action = clap::ArgAction::Append)]
    texts: Vec<String>,

    // A list of queries or keywords to search against
    #[arg(short, long, num_args = 1.., action = clap::ArgAction::Append)]
    queries: Vec<String>,

    // The top-k files or texts to return
    #[arg(long, default_value_t = 3)]
    top_k: i32,
}

struct Document {
  filename: String,
  lines: Vec<String>,
}


fn main() {
    println!("Hello, world!");

    let args = Args::parse();
    for f in args.files {
        println!("file: {:?}", f);
    }

    for q in args.queries {
        println!("query: {:?}", q);
    }

    for t in args.texts {
        println!("text: {:?}", t);
    }
   
    println!("top-k: {:?}", args.top_k);
}
