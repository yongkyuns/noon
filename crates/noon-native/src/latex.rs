//! Native system-TeX and optional pinned browser-engine compiler adapters.
//!
//! Compilation is authoring work in a separate process. Rendering never uses
//! this protocol or keeps a compiler alive on its behalf.
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    sync::mpsc,
    thread::{self, JoinHandle},
    time::Duration,
};

use noon::{DviFontResource, LatexBackend, LatexFormat};
use serde_json::{json, Value};

const MAX_HEADER: usize = 4096;
const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
const MAX_DOCUMENT: usize = 1024 * 1024;
type Response = Result<(Value, Vec<u8>), String>;

static LATEX_JOB_NONCE: AtomicU64 = AtomicU64::new(0);

/// Native system-TeX compiler configuration.
///
/// TeX Live normally supplies TFM metrics but not the OpenType outlines used by
/// Noon's retained text renderer. `font_directory` must therefore contain an
/// explicitly selected `<dvi-name>.ttf` for every font emitted by the document.
/// `resource_identity` identifies that exact compiler/font installation for the
/// shared compiled-resource cache.
#[derive(Clone, Debug)]
pub struct NativeLatexConfig {
    pub latex: PathBuf,
    pub kpsewhich: PathBuf,
    pub font_directory: PathBuf,
    pub resource_identity: String,
    pub timeout: Duration,
}

impl NativeLatexConfig {
    pub fn new(
        latex: impl Into<PathBuf>,
        kpsewhich: impl Into<PathBuf>,
        font_directory: impl Into<PathBuf>,
        resource_identity: impl Into<String>,
    ) -> Self {
        Self {
            latex: latex.into(),
            kpsewhich: kpsewhich.into(),
            font_directory: font_directory.into(),
            resource_identity: resource_identity.into(),
            timeout: Duration::from_secs(45),
        }
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

/// First-class native LaTeX adapter backed by a conventional system executable.
///
/// Compilation is authoring-time subprocess work. DVI normalization and all
/// semantic/resource publication remain in the shared Rust implementation.
pub struct NativeLatexBackend {
    config: NativeLatexConfig,
    identity: String,
    fonts: BTreeMap<String, DviFontResource>,
}

impl NativeLatexBackend {
    pub fn new(config: NativeLatexConfig) -> Result<Self, String> {
        if config.resource_identity.is_empty() || config.resource_identity.len() > 4096 {
            return Err(
                "native LaTeX resource identity must be non-empty and at most 4096 bytes".into(),
            );
        }
        if config.timeout.is_zero() {
            return Err("native LaTeX deadline must be positive".into());
        }
        if !config.font_directory.is_dir() {
            return Err(format!(
                "native LaTeX font directory does not exist: {}",
                config.font_directory.display()
            ));
        }
        let probe_directory = unique_latex_directory("probe")?;
        let probe_cleanup = TempLatexDirectory(probe_directory.clone());
        let mut probe = Command::new(&config.latex);
        probe.arg("--version");
        let version = run_bounded(
            &mut probe,
            config.timeout,
            &probe_directory.join("version.log"),
            64 * 1024,
        )?;
        if !version.success() {
            return Err(format!(
                "native LaTeX compiler version probe failed with {}",
                version
            ));
        }
        let version_line = bounded_file_tail(&probe_directory.join("version.log"), 64 * 1024)
            .lines()
            .next()
            .unwrap_or("unknown")
            .trim()
            .to_owned();
        drop(probe_cleanup);
        let identity = format!(
            "native-latex-v1:{}:{}",
            config.resource_identity, version_line
        );
        Ok(Self {
            config,
            identity,
            fonts: BTreeMap::new(),
        })
    }

    fn validate_font_name(name: &str) -> Result<(), String> {
        if name.is_empty()
            || name.len() > 128
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err("invalid LaTeX font name".into());
        }
        Ok(())
    }
}

impl LatexBackend for NativeLatexBackend {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn format(&self) -> LatexFormat {
        LatexFormat::Article
    }

    fn compile(&mut self, document: &str) -> Result<Vec<u8>, String> {
        if document.len() > MAX_DOCUMENT {
            return Err("LaTeX document exceeds 1 MiB".into());
        }
        let directory = unique_latex_directory("compile")?;
        let cleanup = TempLatexDirectory(directory.clone());
        let source = directory.join("noon.tex");
        fs::write(&source, document)
            .map_err(|error| format!("cannot write native LaTeX source: {error}"))?;
        let mut command = Command::new(&self.config.latex);
        command
            .arg("-interaction=nonstopmode")
            .arg("-halt-on-error")
            .arg("-no-shell-escape")
            .arg("-output-directory")
            .arg(&directory)
            .arg(&source);
        let output = run_bounded(
            &mut command,
            self.config.timeout,
            &directory.join("console.log"),
            64 * 1024,
        )?;
        if !output.success() {
            let diagnostic = bounded_file_tail(&directory.join("noon.log"), 64 * 1024);
            let tail = diagnostic.lines().rev().take(12).collect::<Vec<_>>();
            return Err(format!(
                "native LaTeX compilation failed with {}: {}",
                output,
                tail.into_iter().rev().collect::<Vec<_>>().join("\n")
            ));
        }
        let dvi = fs::read(directory.join("noon.dvi"))
            .map_err(|error| format!("native LaTeX did not produce noon.dvi: {error}"))?;
        drop(cleanup);
        if dvi.len() > MAX_PAYLOAD {
            return Err("native LaTeX DVI exceeds 16 MiB".into());
        }
        Ok(dvi)
    }

    fn font(&mut self, name: &str) -> Result<DviFontResource, String> {
        if let Some(font) = self.fonts.get(name) {
            return Ok(font.clone());
        }
        Self::validate_font_name(name)?;
        if self.fonts.len() >= 256 {
            return Err("LaTeX font resource limit exceeded".into());
        }
        let directory = unique_latex_directory("kpsewhich")?;
        let cleanup = TempLatexDirectory(directory.clone());
        let output_path = directory.join("path.txt");
        let mut command = Command::new(&self.config.kpsewhich);
        command.arg(format!("{name}.tfm"));
        let status = run_bounded(&mut command, self.config.timeout, &output_path, 4096)?;
        if !status.success() {
            return Err(format!("kpsewhich failed for {name}.tfm with {status}"));
        }
        let tfm_path = bounded_file_tail(&output_path, 4096).trim().to_owned();
        drop(cleanup);
        if !tfm_path.is_empty() && !Path::new(&tfm_path).is_file() {
            return Err(format!(
                "kpsewhich returned a missing TFM for {name}: {tfm_path}"
            ));
        }
        if tfm_path.is_empty() {
            return Err(format!(
                "system TeX cannot resolve required metric {name}.tfm"
            ));
        }
        let ttf_path = self.config.font_directory.join(format!("{name}.ttf"));
        if !ttf_path.is_file() {
            return Err(format!(
                "native LaTeX requires explicit outline asset {} for DVI font {name}",
                ttf_path.display()
            ));
        }
        let tfm = fs::read(&tfm_path)
            .map_err(|error| format!("cannot read native LaTeX metric {tfm_path}: {error}"))?;
        let ttf = fs::read(&ttf_path).map_err(|error| {
            format!(
                "cannot read native LaTeX outline {}: {error}",
                ttf_path.display()
            )
        })?;
        let font = DviFontResource::bakoma(name, &self.identity, tfm, ttf)
            .map_err(|error| error.to_string())?;
        self.fonts.insert(name.to_owned(), font.clone());
        Ok(font)
    }
}

struct TempLatexDirectory(PathBuf);

fn unique_latex_directory(purpose: &str) -> Result<PathBuf, String> {
    let nonce = LATEX_JOB_NONCE.fetch_add(1, Ordering::Relaxed);
    let directory = std::env::temp_dir().join(format!(
        "noon-native-latex-{purpose}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory)
        .map_err(|error| format!("cannot create native LaTeX work directory: {error}"))?;
    Ok(directory)
}

fn run_bounded(
    command: &mut Command,
    timeout: Duration,
    output: &Path,
    max_output: u64,
) -> Result<ExitStatus, String> {
    let deadline = std::time::Instant::now()
        .checked_add(timeout)
        .ok_or("native LaTeX deadline is too large")?;
    let stdout = fs::File::create(output)
        .map_err(|error| format!("cannot create native LaTeX diagnostic file: {error}"))?;
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout))
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("cannot start native LaTeX process: {error}"))?;
    loop {
        if fs::metadata(output).map(|value| value.len()).unwrap_or(0) > max_output {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "native LaTeX process output exceeded {max_output} bytes and was terminated"
            ));
        }
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("cannot poll native LaTeX process: {error}"));
            }
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err("native LaTeX process exceeded its deadline and was terminated".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn bounded_file_tail(path: &Path, maximum: usize) -> String {
    let Ok(mut file) = fs::File::open(path) else {
        return String::new();
    };
    let length = file.metadata().map(|value| value.len()).unwrap_or(0);
    if length > maximum as u64 {
        use std::io::Seek;
        let _ = file.seek(std::io::SeekFrom::End(-(maximum as i64)));
    }
    let mut bytes = Vec::with_capacity(maximum.min(length as usize));
    let _ = file.take(maximum as u64).read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

impl Drop for TempLatexDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Explicit native LaTeX host. Requires Node.js and `web/latex/native-host.mjs`.
/// Pinned compiler assets are prepared only when [`Self::start`] is called.
/// Dropping the host or exceeding its deadline kills and reaps the child.
pub struct NodeLatexBackend {
    child: Child,
    requests: Option<mpsc::SyncSender<Vec<u8>>>,
    responses: Option<mpsc::Receiver<Response>>,
    reader: Option<JoinHandle<()>>,
    identity: String,
    timeout: Duration,
    fonts: BTreeMap<String, DviFontResource>,
}

impl NodeLatexBackend {
    pub fn start(node: &Path, script: &Path, timeout: Duration) -> Result<Self, String> {
        if timeout.is_zero() {
            return Err("LaTeX host deadline must be positive".into());
        }
        let mut child = Command::new(node)
            .arg(script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("Cannot start LaTeX host: {error}"))?;
        let mut input = child.stdin.take().expect("piped child stdin");
        let mut output = child.stdout.take().expect("piped child stdout");
        let (sender, responses) = mpsc::sync_channel(1);
        let (requests, receiver) = mpsc::sync_channel::<Vec<u8>>(1);
        // Both pipe writes and reads belong to this worker: a compiler that
        // stops consuming a large document must not defeat the host deadline.
        let reader = thread::spawn(move || {
            let hello = read_response(&mut output);
            let failed = hello.is_err();
            if sender.send(hello).is_err() || failed {
                return;
            }
            while let Ok(bytes) = receiver.recv() {
                let response = input
                    .write_all(&bytes)
                    .and_then(|()| input.flush())
                    .map_err(|error| format!("Cannot send LaTeX request: {error}"))
                    .and_then(|()| read_response(&mut output));
                let failed = response.is_err();
                if sender.send(response).is_err() || failed {
                    break;
                }
            }
        });
        let mut host = Self {
            child,
            requests: Some(requests),
            responses: Some(responses),
            reader: Some(reader),
            identity: String::new(),
            timeout,
            fonts: BTreeMap::new(),
        };
        let (hello, payload) = host.receive()?;
        if hello["protocol"].as_u64() != Some(1) || !payload.is_empty() {
            return Err("Invalid LaTeX host protocol".into());
        }
        host.identity = hello["identity"]
            .as_str()
            .filter(|identity| !identity.is_empty())
            .ok_or("Missing LaTeX compiler identity")?
            .to_owned();
        Ok(host)
    }

    fn stop(&mut self) {
        self.requests.take();
        self.responses.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }

    fn receive(&mut self) -> Response {
        let result = self
            .responses
            .as_ref()
            .ok_or("LaTeX host has stopped")?
            .recv_timeout(self.timeout);
        match result {
            Ok(Ok((header, payload))) => {
                if let Some(error) = header["error"].as_str() {
                    Err(error.to_owned())
                } else {
                    Ok((header, payload))
                }
            }
            Ok(Err(error)) => {
                self.stop();
                Err(error)
            }
            Err(error) => {
                self.stop();
                Err(format!(
                    "LaTeX host did not respond before its deadline: {error}"
                ))
            }
        }
    }

    fn request(&mut self, request: Value) -> Response {
        if self.responses.is_none() {
            return Err("LaTeX host has stopped".into());
        }
        let mut bytes = serde_json::to_vec(&request).map_err(|error| error.to_string())?;
        bytes.push(b'\n');
        if let Err(error) = self
            .requests
            .as_ref()
            .ok_or("LaTeX host has stopped")?
            .try_send(bytes)
        {
            self.stop();
            return Err(format!("Cannot queue LaTeX request: {error}"));
        }
        self.receive()
    }
}

impl Drop for NodeLatexBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

impl LatexBackend for NodeLatexBackend {
    fn identity(&self) -> &str {
        &self.identity
    }
    fn format(&self) -> LatexFormat {
        LatexFormat::Preloaded
    }

    fn compile(&mut self, document: &str) -> Result<Vec<u8>, String> {
        if document.len() > MAX_DOCUMENT {
            return Err("LaTeX document exceeds 1 MiB".into());
        }
        let (header, payload) = self.request(json!({"op": "compile", "document": document}))?;
        if header["kind"].as_str() != Some("dvi") {
            return Err("Expected a DVI response from LaTeX host".into());
        }
        Ok(payload)
    }

    fn font(&mut self, name: &str) -> Result<DviFontResource, String> {
        if let Some(font) = self.fonts.get(name) {
            return Ok(font.clone());
        }
        if name.len() > 128
            || name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"_-".contains(&byte))
        {
            return Err("Invalid LaTeX font name".into());
        }
        if self.fonts.len() >= 256 {
            return Err("LaTeX font resource limit exceeded".into());
        }
        let (header, payload) = self.request(json!({"op": "font", "name": name}))?;
        if header["kind"].as_str() != Some("font") {
            return Err("Expected a LaTeX font response".into());
        }
        let split = header["tfmBytes"]
            .as_u64()
            .and_then(|n| usize::try_from(n).ok())
            .filter(|n| *n > 0 && *n < payload.len())
            .ok_or("Invalid LaTeX font payload")?;
        let face_key = header["faceKey"]
            .as_str()
            .ok_or("Missing LaTeX font identity")?;
        let font =
            DviFontResource::bakoma(name, &self.identity, &payload[..split], &payload[split..])
                .map_err(|error| error.to_string())?;
        if font.face_key.as_ref() != face_key {
            return Err("LaTeX font identity does not match compiler assets".into());
        }
        self.fonts.insert(name.to_owned(), font.clone());
        Ok(font)
    }
}

fn read_response(input: &mut impl Read) -> Response {
    let mut length = [0; 4];
    input
        .read_exact(&mut length)
        .map_err(|error| format!("LaTeX host closed: {error}"))?;
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_HEADER {
        return Err("Invalid LaTeX host header length".into());
    }
    let mut header = vec![0; length];
    input
        .read_exact(&mut header)
        .map_err(|error| error.to_string())?;
    let header: Value = serde_json::from_slice(&header).map_err(|error| error.to_string())?;
    let length = header["bytes"]
        .as_u64()
        .and_then(|n| usize::try_from(n).ok())
        .filter(|n| *n <= MAX_PAYLOAD)
        .ok_or("Invalid LaTeX host payload length")?;
    let mut payload = vec![0; length];
    input
        .read_exact(&mut payload)
        .map_err(|error| error.to_string())?;
    Ok((header, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn system_backend_from_environment() -> Option<NativeLatexBackend> {
        let font_directory = std::env::var_os("NOON_LATEX_FONT_DIR")?;
        let resource_identity = std::env::var("NOON_LATEX_RESOURCE_IDENTITY").ok()?;
        NativeLatexBackend::new(NativeLatexConfig::new(
            std::env::var_os("NOON_LATEX_EXECUTABLE")
                .unwrap_or_else(|| "/Library/TeX/texbin/latex".into()),
            std::env::var_os("NOON_KPSEWHICH_EXECUTABLE")
                .unwrap_or_else(|| "/Library/TeX/texbin/kpsewhich".into()),
            font_directory,
            resource_identity,
        ))
        .ok()
    }

    #[test]
    fn native_system_latex_compiles_and_normalizes_real_dvi_when_assets_are_supplied() {
        let Some(mut backend) = system_backend_from_environment() else {
            eprintln!(
                "native LaTeX qualification unavailable: set NOON_LATEX_FONT_DIR and NOON_LATEX_RESOURCE_IDENTITY"
            );
            return;
        };
        let mut scene = noon::Scene::new();
        let object = scene
            .math_tex(
                noon::MathTex::new(r"x^2+\frac{1}{2}").unwrap(),
                &mut backend,
            )
            .unwrap();
        assert!(object.width().unwrap() > 0.0);
        let handle = object.state().unwrap().content.text().unwrap();
        let store = scene.integration_store().borrow();
        let resource = store.text_resources().get(handle).unwrap();
        assert_eq!(resource.kind, noon_core::TextSourceKind::MathTex);
        assert!(!resource.runs.is_empty());
        assert!(
            !resource.vector_items.is_empty(),
            "fraction rule was not retained"
        );
        resource.validate().unwrap();
    }

    #[test]
    fn native_system_latex_reports_missing_explicit_outline_asset() {
        let latex = Path::new("/Library/TeX/texbin/latex");
        let kpsewhich = Path::new("/Library/TeX/texbin/kpsewhich");
        if !latex.is_file() || !kpsewhich.is_file() {
            eprintln!("system TeX unavailable; skipping installed-toolchain diagnostic");
            return;
        }
        let directory = std::env::temp_dir().join(format!(
            "noon-native-latex-empty-fonts-{}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).unwrap();
        let mut backend = NativeLatexBackend::new(NativeLatexConfig::new(
            latex,
            kpsewhich,
            &directory,
            "empty-font-fixture-v1",
        ))
        .unwrap();
        let error = backend.font("cmr10").unwrap_err();
        fs::remove_dir_all(directory).unwrap();
        assert!(error.contains("explicit outline asset"), "{error}");
        assert!(error.contains("cmr10.ttf"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn native_process_deadline_kills_and_reaps_child() {
        let directory = unique_latex_directory("deadline-test").unwrap();
        let cleanup = TempLatexDirectory(directory.clone());
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exec sleep 30"]);
        let started = std::time::Instant::now();
        let error = run_bounded(
            &mut command,
            Duration::from_millis(40),
            &directory.join("output.log"),
            4096,
        )
        .unwrap_err();
        drop(cleanup);
        assert!(error.contains("deadline"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(unix)]
    #[test]
    fn native_process_output_limit_kills_and_reaps_child() {
        let directory = unique_latex_directory("output-test").unwrap();
        let cleanup = TempLatexDirectory(directory.clone());
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "while :; do printf 0123456789; done"]);
        let error = run_bounded(
            &mut command,
            Duration::from_secs(2),
            &directory.join("output.log"),
            1024,
        )
        .unwrap_err();
        drop(cleanup);
        assert!(error.contains("output exceeded"), "{error}");
    }

    #[test]
    fn bounded_protocol_rejects_large_or_truncated_messages() {
        assert!(read_response(&mut (MAX_HEADER as u32 + 1).to_be_bytes().as_slice()).is_err());
        for header in [
            json!({"bytes": MAX_PAYLOAD + 1}),
            json!({"bytes": -1}),
            json!({"bytes": 2}),
        ] {
            let header = serde_json::to_vec(&header).unwrap();
            let mut frame = (header.len() as u32).to_be_bytes().to_vec();
            frame.extend(header);
            assert!(read_response(&mut frame.as_slice()).is_err());
        }
    }
    #[test]
    fn bounded_protocol_preserves_binary_payload() {
        let header = br#"{"bytes":3,"kind":"dvi"}"#;
        let mut frame = (header.len() as u32).to_be_bytes().to_vec();
        frame.extend(header);
        frame.extend([0, 255, 10]);
        let (header, bytes) = read_response(&mut frame.as_slice()).unwrap();
        assert_eq!(header["kind"], "dvi");
        assert_eq!(bytes, [0, 255, 10]);
    }
    #[cfg(unix)]
    #[test]
    fn deadline_terminates_and_reaps_an_unresponsive_compiler() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let script = std::env::temp_dir().join(format!(
            "noon-latex-timeout-{}-{nonce}.sh",
            std::process::id()
        ));
        let hello = br#"{"protocol":1,"identity":"deadline-fixture","bytes":0}"#;
        let mut frame = (hello.len() as u32).to_be_bytes().to_vec();
        frame.extend(hello);
        let octal = frame
            .iter()
            .map(|byte| format!("\\{byte:03o}"))
            .collect::<String>();
        std::fs::write(&script, format!("printf '{octal}'\nexec sleep 30\n")).unwrap();
        let result = NodeLatexBackend::start(Path::new("/bin/sh"), &script, Duration::from_secs(2));
        std::fs::remove_file(script).unwrap();
        let mut host = result.unwrap();
        host.timeout = Duration::from_millis(50);
        assert!(host
            .compile(&"x".repeat(MAX_DOCUMENT))
            .unwrap_err()
            .contains("deadline"));
        assert!(host.child.try_wait().unwrap().is_some());
        assert!(host.reader.is_none());
        assert!(host.compile("document").unwrap_err().contains("stopped"));
    }

    #[test]
    #[ignore = "explicit real-engine qualification downloads integrity-pinned compiler assets"]
    fn real_engine_recovers_after_invalid_source_and_reuses_exact_fonts() {
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/latex/native-host.mjs");
        let mut host =
            NodeLatexBackend::start(Path::new("node"), &script, Duration::from_secs(45)).unwrap();
        let document = r"\usepackage{amsmath}\begin{document}\setbox0=\vbox{\hsize=15cm\begin{align*}x^2+\frac{1}{2}\end{align*}}\shipout\box0\end{document}";
        let first = host.compile(document).unwrap();
        assert_eq!(&first[..2], &[247, 2]);
        assert!(host
            .compile(&document.replace("x^2", r"\NoonUnknownCommand"))
            .is_err());
        assert_eq!(host.compile(document).unwrap(), first);
        let font = host.font("cmr10").unwrap();
        assert!(!font.tfm.is_empty());
        assert!(Arc::ptr_eq(&font.ttf, &host.font("cmr10").unwrap().ttf));
        let mut scene = noon::Scene::new();
        // Pinned ManimCE 0.21/dvisvgm outline widths, in scene units. BaKoMa
        // outlines differ slightly from Type 1 outlines, but not by font scale.
        for (source, size, expected) in [
            (r"x^2+\frac{1}{2}", 64.0, 1.8818475333333329),
            (
                r"\alpha+\Gamma+\sum_{i=1}^{3}i+\sqrt{2}",
                48.0,
                4.076218249999999,
            ),
        ] {
            let object = scene
                .math_tex(
                    noon::MathTex::new(source).unwrap().with_font_size(size),
                    &mut host,
                )
                .unwrap();
            assert!(
                (object.width().unwrap() - expected).abs() < 0.003,
                "LaTeX outline width differs from pinned Manim: {} vs {expected}",
                object.width().unwrap()
            );
        }
        let title = scene
            .tex(
                noon::Tex::new(r"Real \LaTeX{} in Noon")
                    .unwrap()
                    .with_font_size(38.0),
                &mut host,
            )
            .unwrap();
        assert!((title.width().unwrap() - 3.4055775354166675).abs() < 0.003);
        for strings in [
            vec!["x^2", "+", r"\frac{1}{2}"],
            vec!["{{ a^{b^{c}} }}", "+", "z"],
        ] {
            let spec = noon::MathTex::from_strings(strings).unwrap();
            let expected_source = spec.source().to_owned();
            let expected_parts = spec.part_spans().to_vec();
            let object = scene.math_tex(spec, &mut host).unwrap();
            assert_eq!(object.text_source().unwrap().as_ref(), expected_source);
            let handle = object.state().unwrap().content.text().unwrap();
            let store = scene.integration_store().borrow();
            let resource = store.text_resources().get(handle).unwrap();
            assert_eq!(resource.source.as_ref(), expected_source);
            assert_eq!(
                resource
                    .parts
                    .iter()
                    .map(|part| part.source_span)
                    .collect::<Vec<_>>(),
                expected_parts,
                "real compiler specials preserve argument and balanced isolation spans"
            );
            assert!(resource.parts.iter().all(|part| part.cluster_count > 0));
            resource.validate().unwrap();
        }
        drop(host);
    }
}
