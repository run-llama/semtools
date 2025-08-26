use anyhow::Result;
use clap::Parser;
use model2vec_rs::model::StaticModel;
use simsimd::SpatialSimilarity;
use std::cmp::{max, min};
use std::fs::read_to_string;

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
    query: Vec<String>,

    // How many lines before/after to return as context
    #[arg(short, long, default_value_t = 3)]
    context: usize,

    // The top-k files or texts to return
    #[arg(long, default_value_t = 3)]
    top_k: usize,
}

pub struct Document {
    filename: String,
    lines: Vec<String>,
    embeddings: Vec<Vec<f32>>,
}

pub struct SearchResult<'a> {
    filename: &'a String,
    lines: &'a [String],
    start: usize,
    end: usize,
    distance: f64,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let model = StaticModel::from_pretrained(
        "minishlab/potion-multilingual-128M", // "minishlab/potion-multilingual-128M",
        None,                                 // Optional: Hugging Face API token for private models
        None, // Optional: bool to override model's default normalization. `None` uses model's config.
        None, // Optional: subfolder if model files are not at the root of the repo/path
    )?;

    let query_embeddings = model.encode_with_args(&args.query, Some(2048), 1024);

    let mut documents = Vec::new();
    for f in args.files {
        let content = read_to_string(&f)?;
        let lines: Vec<&str> = content.lines().collect();

        if lines.is_empty() {
            continue;
        }

        let owned_lines: Vec<String> = lines.iter().map(|s| s.to_string()).collect();

        let embeddings = model.encode_with_args(&owned_lines, Some(2048), 1024);
        documents.push(Document {
            filename: f,
            lines: owned_lines,
            embeddings,
        })
    }

    let mut search_results = Vec::new();
    for doc in &documents {
        for (idx, line_embedding) in doc.embeddings.iter().enumerate() {
            let distance = f32::cosine(&query_embeddings[0], line_embedding);
            if let Some(distance) = distance {
                if distance < 0.5 {
                    let bottom_range = max(0, idx - 3);
                    let top_range = min(doc.lines.len(), idx + 3);
                    search_results.push(SearchResult {
                        filename: &doc.filename,
                        lines: &doc.lines[bottom_range..top_range],
                        distance,
                        start: bottom_range,
                        end: top_range,
                    })
                }
            }
        }
    }

    for search_result in search_results {
        let filename = search_result.filename.to_string();
        let lines_str = search_result.lines.join("\n");
        let distance = search_result.distance;
        let start = search_result.start;
        let end = search_result.end;

        let message = format!("{filename}:{start}::{end} ({distance})\n{lines_str}\n\n");
        println!("{}", message);
    }

    Ok(())
}
