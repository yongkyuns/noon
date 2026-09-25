//! Native system-TeX compiler adapter.
//!
//! Compilation is authoring work in a separate process. Rendering never keeps
//! a compiler alive on its behalf.
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Duration,
};

use noon::{DviFontResource, LatexBackend, LatexFormat};

const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
const MAX_DOCUMENT: usize = 1024 * 1024;
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

#[cfg(test)]
mod tests {
    use super::*;

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
        drop(store);

        let store = std::rc::Rc::new(std::cell::RefCell::new(noon_core::SemanticStore::new()));
        let parts = noon::LatexParts::from_math_tex(
            std::rc::Rc::clone(&store),
            noon::MathTex::new(r"x^2+\frac{1}{2}")
                .unwrap()
                .with_font_size(36.0),
            &mut backend,
        )
        .unwrap();
        assert!((parts.current_font_size().unwrap() - 36.0).abs() < 1e-6);
        let mut first = parts.current_members().unwrap().remove(0);
        first.scale(2.0, 2.0).unwrap();
        assert!((parts.current_font_size().unwrap() - 72.0).abs() < 1e-5);

        let scene = noon::Scene::new();
        let mut execution = scene.execution_session().unwrap();
        let mut live = scene.live(&mut execution);
        let live_parts = live
            .create_math_tex_parts(
                noon::MathTex::new(r"x^2+\frac{1}{2}")
                    .unwrap()
                    .with_isolated_substrings(["x", r"\frac{1}{2}"])
                    .unwrap()
                    .with_font_size(36.0),
                &mut backend,
            )
            .unwrap();
        assert_eq!(live_parts.current_members().unwrap().len(), 3);
        live.add_many(&[live_parts.family().into()]).unwrap();
        assert!((live.latex_font_size(&live_parts).unwrap() - 36.0).abs() < 1e-6);

        let target = live.copy_family(live_parts.family()).unwrap();
        live.scale_family(target.root(), 2.0, 2.0).unwrap();
        let segment = live
            .declare_and_activate_family_transform_to(
                live_parts.family(),
                target.root(),
                noon::AnimationOptions::new()
                    .run_time(1.0)
                    .rate_func(noon::RateFunction::Linear),
            )
            .unwrap();
        live.advance_segment_to(segment, segment.start_time() + 0.5)
            .unwrap();
        assert!((live.latex_font_size(&live_parts).unwrap() - 54.0).abs() < 1e-4);
        live.advance_segment_to(segment, segment.end_time())
            .unwrap();
        live.complete_segment(segment).unwrap();

        let before_rotation = live.effective_family_layout(live_parts.family()).unwrap();
        live.rotate_family(
            live_parts.family(),
            std::f64::consts::FRAC_PI_2,
            noon::ManimRotationPivot::Center,
        )
        .unwrap();
        let rotated_size = live.latex_font_size(&live_parts).unwrap();
        let expected = 72.0 * before_rotation.width / before_rotation.height;
        assert!((rotated_size - expected).abs() < 1e-4);

        let removed = live_parts.current_members().unwrap().remove(0);
        live.remove_family_members(live_parts.family(), &[(&removed).into()])
            .unwrap();
        assert!(live.latex_font_size(&live_parts).unwrap().is_finite());

        let rebound = live_parts.rebind_family(target.root().clone()).unwrap();
        assert!(live.latex_font_size(&rebound).unwrap().is_finite());
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
}
