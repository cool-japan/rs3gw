// Test Data Generator for rs3gw
// Generates various types of test data for S3 gateway testing

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use scirs2_core::random::quick::{random_f64, random_int, random_usize};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;

#[derive(Parser)]
#[clap(name = "testdata-generator")]
#[clap(about = "Generate test data for rs3gw S3 gateway", long_about = None)]
struct Cli {
    #[clap(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate random binary files
    Binary {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Number of files to generate
        #[clap(short, long, default_value = "100")]
        count: usize,

        /// Minimum file size in bytes
        #[clap(long, default_value = "1024")]
        min_size: usize,

        /// Maximum file size in bytes
        #[clap(long, default_value = "1048576")]
        max_size: usize,
    },

    /// Generate CSV files
    Csv {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Number of files to generate
        #[clap(short, long, default_value = "10")]
        count: usize,

        /// Number of rows per file
        #[clap(short, long, default_value = "1000")]
        rows: usize,

        /// Number of columns
        #[clap(long, default_value = "10")]
        columns: usize,
    },

    /// Generate JSON files
    Json {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Number of files to generate
        #[clap(short, long, default_value = "10")]
        count: usize,

        /// Number of objects per file
        #[clap(long, default_value = "100")]
        objects: usize,
    },

    /// Generate Parquet files
    Parquet {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Number of files to generate
        #[clap(short, long, default_value = "5")]
        count: usize,

        /// Number of rows per file
        #[clap(short, long, default_value = "10000")]
        rows: usize,
    },

    /// Generate image files (placeholder data)
    Images {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Number of files to generate
        #[clap(short, long, default_value = "50")]
        count: usize,

        /// Image dimensions (width x height)
        #[clap(long, default_value = "1024")]
        size: u32,
    },

    /// Generate text files with lorem ipsum
    Text {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Number of files to generate
        #[clap(short, long, default_value = "50")]
        count: usize,

        /// Number of paragraphs per file
        #[clap(short, long, default_value = "50")]
        paragraphs: usize,
    },

    /// Generate a complete dataset with mixed file types
    Dataset {
        /// Output directory
        #[clap(short, long, value_parser)]
        output: PathBuf,

        /// Dataset size: small, medium, large, or xlarge
        #[clap(short, long, default_value = "medium")]
        size: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Binary {
            output,
            count,
            min_size,
            max_size,
        } => generate_binary_files(output, count, min_size, max_size).await?,

        Commands::Csv {
            output,
            count,
            rows,
            columns,
        } => generate_csv_files(output, count, rows, columns).await?,

        Commands::Json {
            output,
            count,
            objects,
        } => generate_json_files(output, count, objects).await?,

        Commands::Parquet {
            output,
            count,
            rows,
        } => generate_parquet_files(output, count, rows).await?,

        Commands::Images {
            output,
            count,
            size,
        } => generate_image_files(output, count, size).await?,

        Commands::Text {
            output,
            count,
            paragraphs,
        } => generate_text_files(output, count, paragraphs).await?,

        Commands::Dataset { output, size } => generate_dataset(output, &size).await?,
    }

    Ok(())
}

async fn generate_binary_files(
    output: PathBuf,
    count: usize,
    min_size: usize,
    max_size: usize,
) -> Result<()> {
    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    println!("Generating {} binary files...", count);

    for i in 0..count {
        let size = random_usize(min_size, max_size);
        let filename = format!("binary_{:05}.bin", i);
        let path = output.join(&filename);

        let mut data = vec![0u8; size];
        // Fill with random bytes using getrandom
        getrandom::fill(&mut data).expect("Failed to generate random data");

        fs::write(&path, &data)
            .await
            .context(format!("Failed to write {}", filename))?;

        if (i + 1) % 10 == 0 {
            println!("  Generated {}/{} files", i + 1, count);
        }
    }

    println!("✓ Generated {} binary files in {:?}", count, output);
    Ok(())
}

async fn generate_csv_files(
    output: PathBuf,
    count: usize,
    rows: usize,
    columns: usize,
) -> Result<()> {
    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    println!("Generating {} CSV files...", count);

    for file_idx in 0..count {
        let filename = format!("data_{:05}.csv", file_idx);
        let path = output.join(&filename);
        let mut file = fs::File::create(&path)
            .await
            .context(format!("Failed to create {}", filename))?;

        // Write header
        let header = (0..columns)
            .map(|i| format!("column_{}", i))
            .collect::<Vec<_>>()
            .join(",");
        file.write_all(format!("{}\n", header).as_bytes())
            .await
            .context("Failed to write header")?;

        // Write rows
        for _ in 0..rows {
            let row = (0..columns)
                .map(|_| random_int(0, 9999).to_string())
                .collect::<Vec<_>>()
                .join(",");
            file.write_all(format!("{}\n", row).as_bytes())
                .await
                .context("Failed to write row")?;
        }

        if (file_idx + 1) % 5 == 0 {
            println!("  Generated {}/{} files", file_idx + 1, count);
        }
    }

    println!("✓ Generated {} CSV files in {:?}", count, output);
    Ok(())
}

async fn generate_json_files(output: PathBuf, count: usize, objects: usize) -> Result<()> {
    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    println!("Generating {} JSON files...", count);

    for file_idx in 0..count {
        let filename = format!("data_{:05}.json", file_idx);
        let path = output.join(&filename);

        let mut data = Vec::new();
        for obj_idx in 0..objects {
            let obj = serde_json::json!({
                "id": obj_idx,
                "name": format!("object_{}", obj_idx),
                "value": random_int(0, 9999),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "metadata": {
                    "category": format!("cat_{}", random_int(0, 9)),
                    "priority": random_int(1, 5),
                }
            });
            data.push(obj);
        }

        let json_str = serde_json::to_string_pretty(&data).context("Failed to serialize JSON")?;
        fs::write(&path, json_str)
            .await
            .context(format!("Failed to write {}", filename))?;

        if (file_idx + 1) % 5 == 0 {
            println!("  Generated {}/{} files", file_idx + 1, count);
        }
    }

    println!("✓ Generated {} JSON files in {:?}", count, output);
    Ok(())
}

async fn generate_parquet_files(output: PathBuf, count: usize, rows: usize) -> Result<()> {
    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    println!("Generating {} Parquet files...", count);

    use arrow::array::{Float64Array, Int32Array, StringArray};
    use arrow::datatypes::{DataType, Field, Schema};
    use arrow::record_batch::RecordBatch;
    use parquet::arrow::ArrowWriter;
    use parquet::file::properties::WriterProperties;
    use std::fs::File;
    use std::sync::Arc;

    let schema = Arc::new(Schema::new(vec![
        Field::new("id", DataType::Int32, false),
        Field::new("name", DataType::Utf8, false),
        Field::new("value", DataType::Float64, false),
        Field::new("category", DataType::Utf8, false),
    ]));

    for file_idx in 0..count {
        let filename = format!("data_{:05}.parquet", file_idx);
        let path = output.join(&filename);

        let file = File::create(&path).context(format!("Failed to create {}", filename))?;

        let props = WriterProperties::builder()
            .set_compression(parquet::basic::Compression::SNAPPY)
            .build();

        let mut writer = ArrowWriter::try_new(file, schema.clone(), Some(props))
            .context("Failed to create parquet writer")?;

        // Generate data in batches
        let batch_size = 1000;
        for batch_idx in 0..(rows / batch_size) {
            let ids: Vec<i32> = (0..batch_size)
                .map(|i| (batch_idx * batch_size + i) as i32)
                .collect();
            let names: Vec<String> = (0..batch_size)
                .map(|i| format!("name_{}", batch_idx * batch_size + i))
                .collect();
            let values: Vec<f64> = (0..batch_size).map(|_| random_f64() * 10000.0).collect();
            let categories: Vec<String> = (0..batch_size)
                .map(|_| format!("cat_{}", random_int(0, 9)))
                .collect();

            let batch = RecordBatch::try_new(
                schema.clone(),
                vec![
                    Arc::new(Int32Array::from(ids)),
                    Arc::new(StringArray::from(names)),
                    Arc::new(Float64Array::from(values)),
                    Arc::new(StringArray::from(categories)),
                ],
            )
            .context("Failed to create record batch")?;

            writer.write(&batch).context("Failed to write batch")?;
        }

        writer.close().context("Failed to close parquet writer")?;

        if (file_idx + 1) % 5 == 0 {
            println!("  Generated {}/{} files", file_idx + 1, count);
        }
    }

    println!("✓ Generated {} Parquet files in {:?}", count, output);
    Ok(())
}

async fn generate_image_files(output: PathBuf, count: usize, size: u32) -> Result<()> {
    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    println!("Generating {} image files (PNG placeholders)...", count);

    for i in 0..count {
        let filename = format!("image_{:05}.png", i);
        let path = output.join(&filename);

        // Generate random pixel data
        let pixel_count = (size * size * 3) as usize;
        let mut data = vec![0u8; pixel_count];
        // Fill with random bytes using getrandom
        getrandom::fill(&mut data).expect("Failed to generate random data");

        // Simple PNG-like header (not a valid PNG, just placeholder)
        let mut file_data = Vec::new();
        file_data.extend_from_slice(b"\x89PNG\r\n\x1a\n"); // PNG signature
        file_data.extend_from_slice(&data);

        fs::write(&path, &file_data)
            .await
            .context(format!("Failed to write {}", filename))?;

        if (i + 1) % 10 == 0 {
            println!("  Generated {}/{} files", i + 1, count);
        }
    }

    println!("✓ Generated {} image files in {:?}", count, output);
    Ok(())
}

async fn generate_text_files(output: PathBuf, count: usize, paragraphs: usize) -> Result<()> {
    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    println!("Generating {} text files...", count);

    const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

    for i in 0..count {
        let filename = format!("text_{:05}.txt", i);
        let path = output.join(&filename);

        let mut content = String::new();
        for p in 0..paragraphs {
            content.push_str(&format!("Paragraph {}:\n", p + 1));
            content.push_str(LOREM);
            content.push_str("\n\n");
        }

        fs::write(&path, content)
            .await
            .context(format!("Failed to write {}", filename))?;

        if (i + 1) % 10 == 0 {
            println!("  Generated {}/{} files", i + 1, count);
        }
    }

    println!("✓ Generated {} text files in {:?}", count, output);
    Ok(())
}

async fn generate_dataset(output: PathBuf, size: &str) -> Result<()> {
    let (binary_count, csv_count, json_count, parquet_count, image_count, text_count) = match size {
        "small" => (10, 5, 5, 2, 10, 10),
        "medium" => (100, 20, 20, 10, 50, 50),
        "large" => (500, 50, 50, 25, 200, 100),
        "xlarge" => (1000, 100, 100, 50, 500, 200),
        _ => {
            println!("Invalid size. Using 'medium'. Valid options: small, medium, large, xlarge");
            (100, 20, 20, 10, 50, 50)
        }
    };

    println!("Generating '{}' dataset in {:?}...", size, output);

    fs::create_dir_all(&output)
        .await
        .context("Failed to create output directory")?;

    // Generate different file types
    generate_binary_files(output.join("binary"), binary_count, 1024, 1048576).await?;
    generate_csv_files(output.join("csv"), csv_count, 1000, 10).await?;
    generate_json_files(output.join("json"), json_count, 100).await?;
    generate_parquet_files(output.join("parquet"), parquet_count, 10000).await?;
    generate_image_files(output.join("images"), image_count, 1024).await?;
    generate_text_files(output.join("text"), text_count, 50).await?;

    println!("\n✓ Dataset generation complete!");
    println!("  Binary files:  {}", binary_count);
    println!("  CSV files:     {}", csv_count);
    println!("  JSON files:    {}", json_count);
    println!("  Parquet files: {}", parquet_count);
    println!("  Image files:   {}", image_count);
    println!("  Text files:    {}", text_count);

    Ok(())
}
