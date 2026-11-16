//! Optional blocking native narration preparation command.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use noon::narration::preparation::native::NarrationPreparer;
use noon::narration::preparation::{
    cues_from_json, PreparationOptions, SpeechRequest, MAX_METADATA_BYTES, MAX_TEXT_BYTES,
    MAX_WAV_BYTES,
};

const USAGE: &str = "Prepare immutable narration before Scene execution.\n\nUsage:\n  prepare_narration --wav RECORDING.wav --spoken-text-file SPOKEN.txt --cache-dir CACHE [--caption-text-file CAPTIONS.txt] [--cues CUES.json]\n  prepare_narration --request REQUEST.json --cache-dir CACHE [--caption-text-file CAPTIONS.txt] [--cues CUES.json]\n\nWAV import accepts bounded mono/stereo PCM16 recordings. The explicit ElevenLabs\nrequest uses ELEVENLABS_API_KEY only on a verified cache miss. Prints the immutable\nmanifest path; no Scene execution, alignment, or automatic retry occurs here.";

#[derive(Default)]
struct Args {
    wav: Option<PathBuf>,
    request: Option<PathBuf>,
    spoken: Option<PathBuf>,
    caption: Option<PathBuf>,
    cues: Option<PathBuf>,
    cache: Option<PathBuf>,
}

fn parse_args() -> Result<Option<Args>, String> {
    let mut parsed = Args::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(flag) = args.next() {
        if flag == "--help" || flag == "-h" {
            return Ok(None);
        }
        let slot = match flag.to_str() {
            Some("--wav") => &mut parsed.wav,
            Some("--request") => &mut parsed.request,
            Some("--spoken-text-file") => &mut parsed.spoken,
            Some("--caption-text-file") => &mut parsed.caption,
            Some("--cues") => &mut parsed.cues,
            Some("--cache-dir") => &mut parsed.cache,
            _ => return Err("unknown argument; use --help for accepted options".into()),
        };
        if slot.is_some() {
            return Err("each option may be supplied only once".into());
        }
        let value = args.next().ok_or("option requires a path")?;
        *slot = Some(PathBuf::from(value));
    }
    if parsed.wav.is_some() == parsed.request.is_some() {
        return Err("choose exactly one --wav or --request".into());
    }
    if parsed.wav.is_some() != parsed.spoken.is_some() {
        return Err(
            "--spoken-text-file is required with --wav and unavailable with --request".into(),
        );
    }
    if parsed.cache.is_none() {
        return Err("--cache-dir is required".into());
    }
    Ok(Some(parsed))
}

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let file = File::open(path)?;
    if file.metadata()?.len() > limit as u64 {
        return Err("input file exceeds its byte limit".into());
    }
    let mut bytes = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        return Err("input file exceeds its byte limit".into());
    }
    Ok(bytes)
}

fn read_text(path: &Path, limit: usize) -> Result<String, Box<dyn std::error::Error>> {
    Ok(String::from_utf8(read_bounded(path, limit)?)?)
}

fn run(args: Args) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let caption_text = args
        .caption
        .as_deref()
        .map(|path| read_text(path, MAX_TEXT_BYTES))
        .transpose()?;
    let cues = if let Some(path) = args.cues {
        cues_from_json(&read_text(&path, MAX_METADATA_BYTES)?)?
    } else {
        Vec::new()
    };
    let options = PreparationOptions { caption_text, cues };
    let preparer = NarrationPreparer::new(args.cache.expect("validated cache path"));
    let prepared = if let Some(path) = args.wav {
        let spoken = read_text(
            args.spoken.as_deref().expect("validated transcript path"),
            MAX_TEXT_BYTES,
        )?;
        let wav = read_bounded(&path, MAX_WAV_BYTES)?;
        preparer.prepare_wav(&wav, spoken, options)?
    } else {
        let request = SpeechRequest::from_json(&read_text(
            args.request.as_deref().expect("validated request path"),
            MAX_METADATA_BYTES,
        )?)?;
        preparer.prepare_speech(&request, options)?
    };
    Ok(prepared.manifest_path)
}

fn main() {
    let result = parse_args()
        .map_err(|error| -> Box<dyn std::error::Error> { error.into() })
        .and_then(|args| {
            if let Some(args) = args {
                run(args).map(Some)
            } else {
                Ok(None)
            }
        });
    match result {
        Ok(Some(path)) => println!("{}", path.display()),
        Ok(None) => println!("{USAGE}"),
        Err(error) => {
            eprintln!("narration preparation failed: {error}");
            std::process::exit(1);
        }
    }
}
