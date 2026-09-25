//! Optional, cancellable host for the same pinned TeX engine used in browsers.
//!
//! Compilation is authoring work in a separate process. Rendering never uses
//! this protocol or keeps a compiler alive on its behalf.
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
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

/// Explicit native LaTeX host. Requires Node.js and `web/latex/native-host.mjs`.
/// Pinned compiler assets are prepared only when [`Self::start`] is called.
/// Dropping the host or exceeding its deadline kills and reaps the child.
pub struct NativeLatexBackend {
    child: Child,
    requests: Option<mpsc::SyncSender<Vec<u8>>>,
    responses: Option<mpsc::Receiver<Response>>,
    reader: Option<JoinHandle<()>>,
    identity: String,
    timeout: Duration,
    fonts: BTreeMap<String, DviFontResource>,
}

impl NativeLatexBackend {
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

impl Drop for NativeLatexBackend {
    fn drop(&mut self) {
        self.stop();
    }
}

impl LatexBackend for NativeLatexBackend {
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
    use noon_core::TextResourceLookup;
    use std::sync::Arc;
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
        let result =
            NativeLatexBackend::start(Path::new("/bin/sh"), &script, Duration::from_secs(2));
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
            NativeLatexBackend::start(Path::new("node"), &script, Duration::from_secs(45)).unwrap();
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
        let session = noon::example_scenes::latex_text::session(&mut host).unwrap();
        drop(host);
        assert_eq!(session.frame().objects.len(), 3);
        assert!(session
            .frame()
            .objects
            .iter()
            .all(|object| object.text().is_some()));
        assert!(session.frame().objects.iter().any(|object| {
            !session
                .text_resources()
                .get(object.text().unwrap())
                .unwrap()
                .vector_items
                .is_empty()
        }));
    }
}
