use std::str::FromStr;
use std::sync::{OnceLock, mpsc};
use std::thread;

use anyhow::{Result, anyhow, bail};

const SCRIPT: &str = include_str!("../vendor/mathjax_svg_rs_js/dist/index.js");

#[derive(Clone, Copy, Debug)]
pub struct RenderOptions {
    pub font_size: f64,
    pub display_mode: bool,
}

impl RenderOptions {
    pub const fn new(font_size: f64) -> Self {
        Self {
            font_size,
            display_mode: true,
        }
    }
}

struct Runtime {
    context: boa_engine::Context,
}

impl Runtime {
    fn new() -> Result<Self> {
        let mut context = boa_engine::Context::builder()
            .build()
            .map_err(|error| anyhow!("failed to create JavaScript context: {error}"))?;

        let now = std::time::Instant::now();
        context
            .eval(boa_engine::Source::from_bytes(patched_script().as_bytes()))
            .map_err(|error| anyhow!(error.to_opaque(&mut context).display().to_string()))?;

        let log = boa_engine::object::FunctionObjectBuilder::new(
            context.realm(),
            boa_engine::NativeFunction::from_fn_ptr(|_this, args, ctx| {
                let message = args
                    .get(1)
                    .and_then(|value| value.to_string(ctx).ok())
                    .and_then(|value| value.to_std_string().ok())
                    .unwrap_or_else(|| "unknown MathJax log message".into());
                let level = args
                    .first()
                    .and_then(|value| value.to_u32(ctx).ok())
                    .unwrap_or(2);
                match level {
                    0 => log::trace!("{message}"),
                    1 => log::debug!("{message}"),
                    2 => log::info!("{message}"),
                    3 => log::warn!("{message}"),
                    4 => log::error!("{message}"),
                    _ => log::warn!("MathJax log[{level}]: {message}"),
                }
                Ok(boa_engine::JsValue::undefined())
            }),
        )
        .build();
        context
            .global_object()
            .set(
                boa_engine::property::PropertyKey::String("__host_log".into()),
                log,
                false,
                &mut context,
            )
            .map_err(|error| anyhow!("failed to install MathJax logger: {error}"))?;

        log::debug!(
            "initialized MathJax JavaScript context in {} ms",
            now.elapsed().as_millis()
        );
        Ok(Self { context })
    }

    fn render_svg(&mut self, tex: &str, options: RenderOptions) -> Result<String> {
        if !options.font_size.is_finite() || options.font_size <= 0.0 {
            bail!("font size must be a positive finite number");
        }

        let entry = self
            .context
            .global_object()
            .get(
                boa_engine::property::PropertyKey::String("__entry_renderTeX".into()),
                &mut self.context,
            )
            .map_err(|error| anyhow!("failed to get MathJax render function: {error}"))?;

        let render = entry
            .as_object()
            .ok_or_else(|| anyhow!("MathJax render entrypoint is not a function"))?;

        let result =
            render
                .call(
                    &boa_engine::JsValue::null(),
                    &[
                        boa_engine::JsValue::new(boa_engine::JsString::from_str(tex).map_err(
                            |error| anyhow!("failed to pass TeX to JavaScript: {error}"),
                        )?),
                        boa_engine::JsValue::new(options.font_size),
                        boa_engine::JsValue::new(u8::from(options.display_mode)),
                    ],
                    &mut self.context,
                )
                .map_err(|error| {
                    anyhow!(
                        "failed to render TeX: {}",
                        error.to_opaque(&mut self.context).display()
                    )
                })?;

        result
            .to_string(&mut self.context)
            .map_err(|error| anyhow!("failed to stringify MathJax SVG: {error}"))?
            .to_std_string()
            .map_err(|error| anyhow!("failed to convert MathJax SVG to Rust string: {error}"))
    }
}

enum WorkerMessage {
    Render {
        tex: String,
        options: RenderOptions,
        response: mpsc::SyncSender<Result<String>>,
    },
    Shutdown,
}

struct MathJax {
    sender: mpsc::Sender<WorkerMessage>,
    handle: Option<thread::JoinHandle<()>>,
}

impl MathJax {
    fn new() -> Self {
        let (sender, receiver) = mpsc::channel();
        let handle = thread::Builder::new()
            .name("mathjax-svg".into())
            .stack_size(4 * 1024 * 1024)
            .spawn(move || {
                let mut runtime = Runtime::new().map_err(|error| error.to_string());
                while let Ok(message) = receiver.recv() {
                    match message {
                        WorkerMessage::Render {
                            tex,
                            options,
                            response,
                        } => {
                            let result = match runtime.as_mut() {
                                Ok(runtime) => runtime.render_svg(&tex, options),
                                Err(message) => Err(anyhow!(message.clone())),
                            };
                            let _ = response.send(result);
                        }
                        WorkerMessage::Shutdown => break,
                    }
                }
            })
            .expect("failed to spawn MathJax worker thread");

        Self {
            sender,
            handle: Some(handle),
        }
    }

    fn render_svg(&self, tex: &str, options: RenderOptions) -> Result<String> {
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        self.sender
            .send(WorkerMessage::Render {
                tex: tex.to_owned(),
                options,
                response: response_tx,
            })
            .map_err(|_| anyhow!("MathJax worker thread is unavailable"))?;
        response_rx
            .recv()
            .map_err(|_| anyhow!("MathJax worker thread stopped before rendering finished"))?
    }
}

impl Drop for MathJax {
    fn drop(&mut self) {
        let _ = self.sender.send(WorkerMessage::Shutdown);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

pub fn render_svg(tex: &str, options: RenderOptions) -> Result<String> {
    shared_mathjax().render_svg(tex, options)
}

fn shared_mathjax() -> &'static MathJax {
    static MATHJAX: OnceLock<MathJax> = OnceLock::new();
    MATHJAX.get_or_init(MathJax::new)
}

fn patched_script() -> &'static String {
    static PATCHED: OnceLock<String> = OnceLock::new();
    PATCHED.get_or_init(|| {
        SCRIPT
            .replacen("fontCache:`local`", "fontCache:`none`", 1)
            .replacen("__host_log(e,n)", "globalThis.__host_log?.(e,n)", 1)
            + "\n\
const __mcCssIdPackage = `monocurl-css-id`;\n\
const __mcCssIdMacroMap = `monocurl-css-id-macros`;\n\
function __mcCssId(parser,name){\n\
  const id=parser.GetArgument(name);\n\
  const body=parser.ParseArg(name);\n\
  parser.Push(parser.create(`node`,`mstyle`,[body],{id}));\n\
}\n\
if(!__.parseOptions.packageData.has(__mcCssIdPackage)){\n\
  __.parseOptions.packageData.set(__mcCssIdPackage,{});\n\
  new Bt(__mcCssIdMacroMap,{cssId:__mcCssId});\n\
  __.parseOptions.handlers.add({[E.MACRO]:[__mcCssIdMacroMap]},{},10);\n\
}\n"
    })
}

#[cfg(test)]
mod tests {
    use super::{RenderOptions, patched_script, render_svg};

    #[test]
    fn patched_script_uses_global_host_logger_lookup() {
        let script = patched_script();
        assert!(script.contains("globalThis.__host_log?.(e,n)"));
        assert!(!script.contains("__host_log(e,n)"));
        assert!(script.contains("monocurl-css-id-macros"));
    }

    #[test]
    fn render_svg_preserves_css_id_markup() {
        let svg = render_svg(r"\cssId{mc-span-lhs}{x}", RenderOptions::new(36.0)).unwrap();
        assert!(
            svg.contains("mc-span-lhs"),
            "expected cssId marker in MathJax output:\n{svg}"
        );
    }
}
