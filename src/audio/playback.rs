use super::super::*;

pub(in crate::app) type AudioMsg = (u64, Result<std::path::PathBuf, String>);
pub(in crate::app) type SrcDec = rodio::Decoder<std::io::BufReader<std::fs::File>>;
pub(in crate::app) type SeekMsg = (u64, f32, Result<SrcDec, String>);

pub(in crate::app) struct Audio {
    pub(in crate::app) stream: Option<(rodio::OutputStream, rodio::OutputStreamHandle)>,
    pub(in crate::app) sink: Option<rodio::Sink>,
    pub(in crate::app) tx: Sender<AudioMsg>,
    pub(in crate::app) rx: Receiver<AudioMsg>,
    pub(in crate::app) ctx: Context,
    pub(in crate::app) gen: u64,
    pub(in crate::app) loading: bool,
    pub(in crate::app) err: Option<String>,
    pub(in crate::app) dur: Option<f32>,
    pub(in crate::app) path: Option<std::path::PathBuf>,
    pub(in crate::app) seek_tx: Sender<SeekMsg>,
    pub(in crate::app) seek_rx: Receiver<SeekMsg>,
    pub(in crate::app) seek_gen: u64,
    pub(in crate::app) pos_base: f32,
    pub(in crate::app) pos_since: Option<std::time::Instant>,
    pub(in crate::app) seek_pending: bool,
}

impl Audio {
    pub(in crate::app) fn new(ctx: Context) -> Self {
        let (tx, rx) = channel();
        let (seek_tx, seek_rx) = channel();
        Self {
            seek_tx,
            seek_rx,
            seek_gen: 0,
            stream: None,
            sink: None,
            tx,
            rx,
            ctx,
            gen: 0,
            loading: false,
            err: None,
            dur: None,
            path: None,
            pos_base: 0.0,
            pos_since: None,
            seek_pending: false,
        }
    }

    pub(in crate::app) fn stop(&mut self) {
        if let Some(s) = self.sink.take() {
            s.stop();
        }
        self.gen += 1;
        self.loading = false;
        self.err = None;
        self.dur = None;
        self.path = None;
        self.seek_gen += 1;
        self.pos_base = 0.0;
        self.pos_since = None;
        self.seek_pending = false;
    }

    pub(in crate::app) fn fetch(&mut self, id: String) {
        self.loading = true;
        let (tx, ctx, g) = (self.tx.clone(), self.ctx.clone(), self.gen);
        std::thread::spawn(move || {
            let r = fetch_audio(&id);
            let _ = tx.send((g, r));
            ctx.request_repaint();
        });
    }

    pub(in crate::app) fn start(
        &mut self,
        path: &std::path::Path,
        playing: bool,
        vol: f32,
        at: f32,
    ) -> Result<(), String> {
        use rodio::Source;
        let src = open_decoder(path, at)?;
        self.dur = src.total_duration().map(|d| d.as_secs_f32()).filter(|d| *d > 1.0);
        self.install(src, path, playing, vol, at)
    }

    pub(in crate::app) fn install(
        &mut self,
        src: SrcDec,
        path: &std::path::Path,
        playing: bool,
        vol: f32,
        at: f32,
    ) -> Result<(), String> {
        if self.stream.is_none() {
            let st = rodio::OutputStream::try_default().map_err(|e| format!("Sortie audio indisponible : {e}"))?;
            self.stream = Some(st);
        }
        let handle = &self.stream.as_ref().unwrap().1;
        if let Some(old) = self.sink.take() {
            old.stop();
        }
        let sink = rodio::Sink::try_new(handle).map_err(|e| e.to_string())?;
        sink.set_volume(vol * vol);
        sink.pause();
        sink.append(src);
        if playing {
            sink.play();
        }
        self.sink = Some(sink);
        self.path = Some(path.to_path_buf());

        self.pos_base = at;
        self.pos_since = if playing { Some(std::time::Instant::now()) } else { None };
        self.seek_pending = false;
        Ok(())
    }

    pub(in crate::app) fn pos(&self) -> f32 {
        self.pos_base + self.pos_since.map_or(0.0, |t| t.elapsed().as_secs_f32())
    }

    pub(in crate::app) fn seek(&mut self, t: f32) {
        let Some(p) = self.path.clone() else { return };
        self.seek_gen += 1;
        self.seek_pending = true;
        let (tx, ctx, g, t) = (self.seek_tx.clone(), self.ctx.clone(), self.seek_gen, t.max(0.0));
        std::thread::spawn(move || {
            let r = open_decoder(&p, t);
            let _ = tx.send((g, t, r));
            ctx.request_repaint();
        });
    }
}

fn open_decoder(path: &std::path::Path, at: f32) -> Result<SrcDec, String> {
    use rodio::Source;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let ogg = path.extension().and_then(|e| e.to_str()) == Some("ogg");

    let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if ogg {
            rodio::Decoder::new_vorbis(std::io::BufReader::new(file))
        } else {
            rodio::Decoder::new_mp4(std::io::BufReader::new(file), rodio::decoder::Mp4Type::M4a)
        }
    }));
    let mut src = match res {
        Ok(Ok(src)) => src,
        other => {
            eprintln!(
                "[decode] {} : {}",
                path.display(),
                match &other {
                    Ok(Err(e)) => e.to_string(),
                    _ => "decoder panic".to_string(),
                }
            );
            let _ = std::fs::remove_file(path);
            cache_lock().files.retain(|_, p| p.as_path() != path);
            return Err(match other {
                Ok(Err(e)) => format!("Unable to play audio: {e}"),
                _ => "Unreadable audio file; it was removed from the cache".to_string(),
            });
        }
    };
    if at > 0.5 {
        let t0 = std::time::Instant::now();

        let native = src.try_seek(std::time::Duration::from_secs_f32(at)).is_ok();
        if !native {
            let n = (at * src.sample_rate() as f32 * src.channels() as f32) as usize;
            if n > 0 {
                let _ = src.nth(n - 1);
            }
        }
        eprintln!(
            "[seek] {} s : {} en {:.2} s",
            at as u32,
            if native { "native" } else { "decoded (slow)" },
            t0.elapsed().as_secs_f32()
        );
    }
    Ok(src)
}
