#[allow(unused_imports)]
use std::env;
#[allow(unused_imports)]
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use sha1::{Digest, Sha1};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Optional name to operate on
    name: Option<String>,

    /// Sets a custom config file
    #[arg(short, long, value_name = "FILE")]
    config: Option<PathBuf>,

    /// Turn debugging information on
    #[arg(short, long, action = clap::ArgAction::Count)]
    debug: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize new repository in current directory
    Init,
    CatFile {
        #[arg(short = 'p')]
        blob_sha: String,
    },
    HashObject {
        #[arg(short = 'w')]
        file: PathBuf,
    },
    LsTree {
        #[clap(long, short, action)]
        name_only: bool,

        sha: String,
    },
    WriteTree,
    Root,
}

fn g_init() -> Result<()> {
    fs::create_dir(".git")?;
    fs::create_dir(".git/objects")?;
    fs::create_dir(".git/refs")?;
    fs::write(".git/HEAD", "ref: refs/heads/master\n")?;
    println!("Initialized git directory");
    Ok(())
}

fn get_repo_root(dir: PathBuf) -> Result<PathBuf> {
    let dir = dir.canonicalize()?;
    let paths = std::fs::read_dir(dir.clone())?
        .into_iter()
        .collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    let mut files: Vec<TreeFile> = Vec::new();
    for path in paths {
        let name = path.file_name();
        if name == ".git" {
            let result: PathBuf = path
                .path()
                .parent()
                .ok_or(anyhow::anyhow!("Error getting repository root directory"))?
                .to_path_buf();
            return Ok(result);
        }
    }
    match dir.parent() {
        Some(path) => get_repo_root(path.to_path_buf()),
        None => {
            anyhow::bail!("Couldn't find git repository here or in any of the parent directories")
        }
    }
}

fn sha_to_path(sha: &str) -> Result<PathBuf> {
    let mut result = get_repo_root(PathBuf::from("."))?;
    result.push(".git");
    result.push("objects");
    result.push(sha.chars().take(2).collect::<String>());
    result.push(sha.chars().skip(2).collect::<String>());
    Ok(result)
}

#[derive(Clone)]
struct Object {
    object_type: ObjectType,
    size: usize,
    content: Vec<u8>,
}

impl Object {
    fn new(object_type: ObjectType, content: String) -> Self {
        Self {
            object_type,
            size: content.len(),
            content: content.as_bytes().to_vec(),
        }
    }
    fn new_blob(content: String) -> Self {
        Self::new(ObjectType::Blob, content)
    }
    fn new_tree(content: Vec<TreeFile>) -> Result<Self> {
        let mut result: Vec<u8> = Vec::new();
        for file in content {
            result.append(&mut format!("{} {}\0", file.mode, file.name).as_bytes().to_vec());
            result.append(&mut hex::decode(file.sha)?);
        }
        Ok(Self {
            object_type: ObjectType::Tree,
            size: result.len(),
            content: result,
        })
    }
    fn hash(&self) -> String {
        let mut hasher = Sha1::new();
        hasher.update(Into::<Vec<u8>>::into(self.clone()));
        hex::encode(hasher.finalize())
    }
    fn compress(&self) -> Result<Vec<u8>> {
        let mut compressor = flate2::read::ZlibEncoder::new(
            std::io::Cursor::new(Into::<Vec<u8>>::into(self.clone())),
            flate2::Compression::fast(),
        );
        let mut result = Vec::new();
        compressor.read_to_end(&mut result)?;
        Ok(result)
    }
}

impl Into<Vec<u8>> for Object {
    fn into(self) -> Vec<u8> {
        let mut r = format!("{0} {1}\0", self.object_type.to_string(), self.size,).into_bytes();
        r.append(&mut self.content.clone());
        r
        // self.content
    }
}

impl TryFrom<Vec<u8>> for Object {
    type Error = anyhow::Error;

    fn try_from(value: Vec<u8>) -> std::result::Result<Self, Self::Error> {
        let mut decompressor = flate2::read::ZlibDecoder::new(&value[..]);
        let mut result: Vec<u8> = Vec::new(); // String::new();
        decompressor.read_to_end(&mut result)?;
        // decompressor.read_to_string(&mut result)?;
        // let mut split = result.split(|&x| x == 0_u8);
        // let header = String::from_utf8(
        //     split
        //         .next()
        //         .ok_or(anyhow::anyhow!("Couldn't parse git object header"))?
        //         .to_vec(),
        // )?;
        let i = result
            .iter()
            .position(|x| *x == 0_u8)
            .ok_or(anyhow::anyhow!("Couldn't parse git object header"))?;
        let header = String::from_utf8(result.drain(0..i).collect())?;
        let content: Vec<u8> = result.into_iter().skip(1).collect();
        // let content: Vec<u8> = split.flatten().map(|&x| x).collect();
        // .next()
        // .ok_or(anyhow::anyhow!("Couldn't parse git object header"))?;
        // let (header, content) = result.split_at(
        //     result
        //         .iter()
        //         .position(|&x| x == 0_u8)
        //         .ok_or(anyhow::anyhow!("Couldn't parse header of object"))?,
        // );
        // let header = String::from_utf8(header.to_vec())?;

        // .split_once(\0)
        // .ok_or(anyhow::anyhow!("Couldn't parse git object"))?;
        let (object_type_str, size_str) = header
            .split_once(" ")
            .ok_or(anyhow::anyhow!("Couldn't parse git object header"))?;
        let size = size_str.parse::<usize>()?;
        let object_type = ObjectType::try_from(object_type_str)?;
        Ok(Self {
            object_type,
            size,
            content,
        })
    }
}

#[derive(Clone)]
enum ObjectType {
    Blob,
    Tree,
    Commit,
}

impl ToString for ObjectType {
    fn to_string(&self) -> String {
        match self {
            ObjectType::Blob => String::from("blob"),
            ObjectType::Tree => String::from("tree"),
            ObjectType::Commit => String::from("commit"),
        }
    }
}

impl TryFrom<&str> for ObjectType {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        match value {
            "blob" => Ok(Self::Blob),
            "tree" => Ok(Self::Tree),
            "commit" => Ok(Self::Commit),
            _ => anyhow::bail!("Couldn't determine object type: {}", value.to_string()),
        }
    }
}

fn p_cat_file(blob_sha: &str) -> Result<()> {
    let path = sha_to_path(blob_sha)?;
    let content = fs::read(path)?;
    let object = Object::try_from(content)?;
    std::io::stdout().write_all(&object.content)?;
    // print!("{}", String::from_utf8(object.content));
    Ok(())
}

fn p_hash_object(file: &PathBuf) -> Result<String> {
    anyhow::ensure!(file.exists());
    let content = fs::read(file)?;
    let object = Object::new_blob(std::string::String::from_utf8(content)?);
    let hash = object.hash();
    let compressed = object.compress()?;
    let path = sha_to_path(&hash)?;
    if path.exists() {
        return Ok(hash);
    }
    std::fs::create_dir_all(&path.parent().ok_or(anyhow::anyhow!("Unreachable"))?)?;
    // println!("{}", path.display());
    let mut file = File::create(path)?;
    file.write_all(&compressed)?;
    // println!("{hash}");
    Ok(hash)
}

struct TreeFile {
    mode: String,
    name: String,
    sha: String,
}

fn p_ls_tree(name_only: bool, sha: &str) -> Result<()> {
    let path = sha_to_path(sha)?;
    let content = fs::read(path)?;
    let mut object = Object::try_from(content)?;
    if !matches!(object.object_type, ObjectType::Tree) {
        anyhow::bail!("Sha doesn't point to tree object");
    }
    let mut files = Vec::new();
    loop {
        let i = match object.content.iter().position(|x| *x == 0_u8) {
            Some(i) => i,
            None => break,
        };
        let file_header_bytes: Vec<u8> = object.content.drain(0..i).collect();
        let file_header: String = String::from_utf8(file_header_bytes)?;
        // let file_header: String =
        //     String::from_utf8(object.content.drain(0..i).collect::<Vec<u8>>().clone())?;
        let (file_mode, file_name) = file_header
            .split_once(" ")
            .ok_or(anyhow::anyhow!("Couldn't parse file header in tree object"))?;
        object.content.drain(0..1);
        let file_sha: Vec<_> = object.content.drain(0..20).collect();
        files.push(TreeFile {
            mode: file_mode.to_string(),
            name: file_name.to_string(),
            sha: hex::encode(file_sha),
        })
        // files.push((file_mode.clone(), file_name.clone(), hex::encode(file_sha)));
        // files.push(String::from_utf8(file_header)?);
    }
    for file in files {
        match name_only {
            true => println!("{}", file.name),
            false => println!("{:0>6} {} {}", file.mode, file.sha, file.name,),
        }
    }
    // dbg!(files);
    Ok(())
}

fn should_ignore(cwd: PathBuf, file: PathBuf) -> Result<bool> {
    let name = file
        .file_name()
        .ok_or(anyhow::anyhow!("Error getting file name"))?;
    if name == ".git" {
        return Ok(true);
    }
    let mut path_gitignore = get_repo_root(cwd.clone())?;
    path_gitignore.push(".gitignore");
    if !path_gitignore.exists() {
        return Ok(false);
    }
    let mut ignores = String::new();
    std::fs::File::open(path_gitignore)?.read_to_string(&mut ignores)?;
    let repo_root = PathBuf::from(get_repo_root(cwd)?);
    let ignores: Vec<_> = ignores
        .split("\n")
        .filter(|l| !l.is_empty() && !l.starts_with("#"))
        .map(|l| {
            if let Some(n) = l.chars().position(|c| c == '#') {
                l.chars().take(n - 1).collect::<String>()
            } else {
                l.to_string()
            }
        })
        .filter(|l| !l.contains("*")) // I won't implement * right now
        .map(|l| {
            let mut pb = repo_root.clone();
            pb.push(l);
            pb.canonicalize()
        })
        .filter_map(|p| p.ok())
        .collect();
    let file_path = file.canonicalize()?;
    for ignore_pattern in ignores {
        if file_path.starts_with(ignore_pattern) {
            return Ok(true);
        }
    }
    // dbg!(ignores, file_path);
    Ok(false)
}

fn p_write_tree(dir: PathBuf) -> Result<String> {
    // dbg!(&dir);
    let paths = std::fs::read_dir(dir.clone())?
        .into_iter()
        .collect::<std::result::Result<Vec<_>, std::io::Error>>()?;
    let mut files: Vec<TreeFile> = Vec::new();
    for path in paths {
        let name = path.file_name();
        // dbg!(&name);
        if should_ignore(dir.clone(), path.path())? {
            // println!("Ignoring {:?}", name);
            continue;
        }
        // if name == ".git" {
        //     continue;
        // }
        let name = name
            .to_str()
            .ok_or(anyhow::anyhow!("Invalid unicode in filename"))?
            .to_string();
        let mode = std::fs::metadata(path.path())?.permissions().mode();
        let mode = format!("{:o}", mode);
        let sha = if path.path().is_dir() {
            // println!("descending into {}", path.path().display());
            p_write_tree(path.path())?
        } else {
            // println!("hashing {}", path.path().display());
            p_hash_object(&path.path())?
        };
        files.push(TreeFile { mode, name, sha });
    }
    files.sort_by(|a, b| a.name.cmp(&b.name));
    let obj = Object::new_tree(files)?;
    let compressed = obj.compress()?;
    let hash = obj.hash();
    let path = sha_to_path(&hash)?;
    if path.exists() {
        return Ok(hash);
    }
    std::fs::create_dir_all(&path.parent().ok_or(anyhow::anyhow!("Unreachable"))?)?;
    let mut file = File::create(path)?;
    file.write_all(&compressed)?;
    Ok(hash)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init => g_init(),
        Commands::CatFile { blob_sha } => p_cat_file(blob_sha),
        Commands::HashObject { file } => {
            println!("{}", p_hash_object(file)?);
            Ok(())
        }
        Commands::LsTree { name_only, sha } => p_ls_tree(*name_only, sha),
        Commands::WriteTree => {
            println!("{}", p_write_tree(PathBuf::from("."))?);
            Ok(())
        }
        Commands::Root => {
            println!("{}", get_repo_root(PathBuf::from("."))?.display());
            Ok(())
        }
    }
}
