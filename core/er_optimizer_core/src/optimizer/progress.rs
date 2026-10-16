use super::*;

#[derive(Clone, Copy, Debug)]
pub(super) struct ProgressEmitState {
    pub(super) last_checked: u64,
    pub(super) last_at: Instant,
}

pub(super) trait SearchProgress {
    fn advance(
        &mut self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
    ) -> Result<(), String>;
    #[cfg(test)]
    fn is_cancelled(&self) -> bool;
    fn poll(&mut self) -> Result<(), String>;
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
}

pub(super) const PARALLEL_PROGRESS_BATCH: u64 = 8_192;
pub(super) const PROGRESS_POLL_BATCH: u32 = 1_024;
pub(super) const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(100);

pub(super) struct SerialSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    pub(super) total: u64,
    pub(super) checked: u64,
    pub(super) eligible: u64,
    pub(super) best_score: Option<f32>,
    pub(super) started: Instant,
    pub(super) progress_every: u64,
    pub(super) last_emit: ProgressEmitState,
    pub(super) callback: F,
    pub(super) cancelled: bool,
    pub(super) poll_count: u32,
}

impl<F> SerialSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    pub(super) fn new(total: u64, progress_every: u64, callback: F) -> Self {
        let started = Instant::now();
        Self {
            total,
            checked: 0,
            eligible: 0,
            best_score: None,
            started,
            progress_every,
            last_emit: ProgressEmitState {
                last_checked: 0,
                last_at: started,
            },
            callback,
            cancelled: false,
            poll_count: 0,
        }
    }

    pub(super) fn emit_initial(&mut self) -> Result<(), String> {
        self.emit(true)
    }

    pub(super) fn emit_final(&mut self) -> Result<(), String> {
        self.emit(true)
    }

    pub(super) fn emit_if_due(&mut self, ignore_count_threshold: bool) -> Result<(), String> {
        if self.progress_every == 0 {
            return Ok(());
        }
        if !ignore_count_threshold
            && self.checked.saturating_sub(self.last_emit.last_checked) < self.progress_every
        {
            return Ok(());
        }
        if self.last_emit.last_at.elapsed() < PROGRESS_MIN_INTERVAL {
            return Ok(());
        }
        self.emit(false)
    }

    pub(super) fn emit(&mut self, force: bool) -> Result<(), String> {
        if self.cancelled && !force {
            return Err("cancelled".to_string());
        }
        let snapshot = ProgressSnapshot {
            checked: self.checked,
            total: self.total,
            eligible: self.eligible,
            best_score: self.best_score.unwrap_or(0.0),
            elapsed_ms: self.started.elapsed().as_millis() as u64,
        };
        if !(self.callback)(snapshot) {
            self.cancelled = true;
            return Err("cancelled".to_string());
        }
        self.last_emit = ProgressEmitState {
            last_checked: self.checked,
            last_at: Instant::now(),
        };
        Ok(())
    }
}

impl<F> SearchProgress for SerialSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool,
{
    fn advance(
        &mut self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
    ) -> Result<(), String> {
        self.checked = self.checked.saturating_add(checked_delta);
        self.eligible = self.eligible.saturating_add(eligible_delta);
        if let Some(score) = best_score
            && self.best_score.is_none_or(|current| score > current)
        {
            self.best_score = Some(score);
        }
        self.emit_if_due(false)
    }

    #[cfg(test)]
    fn is_cancelled(&self) -> bool {
        self.cancelled
    }

    fn poll(&mut self) -> Result<(), String> {
        if self.cancelled {
            return Err("cancelled".to_string());
        }
        if self.progress_every == 0 {
            return Ok(());
        }
        self.poll_count = self.poll_count.saturating_add(1);
        if self.poll_count < PROGRESS_POLL_BATCH {
            return Ok(());
        }
        self.poll_count = 0;
        self.emit_if_due(true)
    }
}

pub(super) struct ParallelSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    pub(super) total: u64,
    pub(super) checked: AtomicU64,
    pub(super) eligible: AtomicU64,
    pub(super) cancelled: AtomicBool,
    pub(super) best_score: Mutex<Option<f32>>,
    pub(super) started: Instant,
    pub(super) progress_every: u64,
    pub(super) last_emit: Mutex<ProgressEmitState>,
    pub(super) callback: Mutex<F>,
}

impl<F> ParallelSearchProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    pub(super) fn new(total: u64, progress_every: u64, callback: F) -> Self {
        let started = Instant::now();
        Self {
            total,
            checked: AtomicU64::new(0),
            eligible: AtomicU64::new(0),
            cancelled: AtomicBool::new(false),
            best_score: Mutex::new(None),
            started,
            progress_every,
            last_emit: Mutex::new(ProgressEmitState {
                last_checked: 0,
                last_at: started,
            }),
            callback: Mutex::new(callback),
        }
    }

    pub(super) fn record(
        &self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
        force: bool,
    ) -> Result<(), String> {
        if checked_delta > 0 {
            self.checked.fetch_add(checked_delta, Ordering::Relaxed);
        }
        if eligible_delta > 0 {
            self.eligible.fetch_add(eligible_delta, Ordering::Relaxed);
        }
        if let Some(score) = best_score {
            let mut guard = self
                .best_score
                .lock()
                .map_err(|_| "failed to lock progress best score".to_string())?;
            if guard.is_none_or(|current| score > current) {
                *guard = Some(score);
            }
        }
        self.emit_if_due(force, false)
    }

    pub(super) fn emit_initial(&self) -> Result<(), String> {
        self.emit_if_due(true, false)
    }

    pub(super) fn emit_final(&self) -> Result<(), String> {
        self.emit_if_due(true, false)
    }

    pub(super) fn emit_if_due(
        &self,
        force: bool,
        ignore_count_threshold: bool,
    ) -> Result<(), String> {
        if self.cancelled.load(Ordering::Relaxed) && !force {
            return Err("cancelled".to_string());
        }
        if !force {
            if self.progress_every == 0 {
                return Ok(());
            }
            let mut emit_guard = self
                .last_emit
                .lock()
                .map_err(|_| "failed to lock progress emit state".to_string())?;
            let checked = self.checked.load(Ordering::Relaxed);
            if !ignore_count_threshold
                && checked.saturating_sub(emit_guard.last_checked) < self.progress_every
            {
                return Ok(());
            }
            if emit_guard.last_at.elapsed() < PROGRESS_MIN_INTERVAL {
                return Ok(());
            }
            *emit_guard = ProgressEmitState {
                last_checked: checked,
                last_at: Instant::now(),
            };
        } else if let Ok(mut emit_guard) = self.last_emit.lock() {
            *emit_guard = ProgressEmitState {
                last_checked: self.checked.load(Ordering::Relaxed),
                last_at: Instant::now(),
            };
        }

        let should_continue = {
            let mut callback = self
                .callback
                .lock()
                .map_err(|_| "failed to lock progress callback".to_string())?;
            // Capture after acquiring delivery order so a delayed emitter cannot
            // send an older snapshot after a newer worker's notification.
            let best_score = self
                .best_score
                .lock()
                .map_err(|_| "failed to lock progress best score".to_string())?
                .unwrap_or(0.0);
            let eligible = self.eligible.load(Ordering::Relaxed);
            let snapshot = ProgressSnapshot {
                checked: self.checked.load(Ordering::Relaxed),
                total: self.total,
                eligible,
                best_score,
                elapsed_ms: self.started.elapsed().as_millis() as u64,
            };
            (callback)(snapshot)
        };
        if !should_continue {
            self.cancelled.store(true, Ordering::Relaxed);
            return Err("cancelled".to_string());
        }
        Ok(())
    }

    pub(super) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

pub(super) struct ParallelLocalProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    pub(super) shared: Arc<ParallelSearchProgress<F>>,
    pub(super) checked: u64,
    pub(super) eligible: u64,
    pub(super) best_score: Option<f32>,
    pub(super) poll_count: u32,
}

impl<F> ParallelLocalProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    pub(super) fn new(shared: Arc<ParallelSearchProgress<F>>) -> Self {
        Self {
            shared,
            checked: 0,
            eligible: 0,
            best_score: None,
            poll_count: 0,
        }
    }

    pub(super) fn flush(&mut self, force: bool) -> Result<(), String> {
        if self.checked == 0 && self.eligible == 0 && self.best_score.is_none() && !force {
            return Ok(());
        }
        let checked = self.checked;
        let eligible = self.eligible;
        let best_score = self.best_score;
        self.checked = 0;
        self.eligible = 0;
        self.best_score = None;
        self.shared.record(checked, eligible, best_score, force)
    }
}

impl<F> SearchProgress for ParallelLocalProgress<F>
where
    F: FnMut(ProgressSnapshot) -> bool + Send,
{
    fn advance(
        &mut self,
        checked_delta: u64,
        eligible_delta: u64,
        best_score: Option<f32>,
    ) -> Result<(), String> {
        self.checked = self.checked.saturating_add(checked_delta);
        self.eligible = self.eligible.saturating_add(eligible_delta);
        if let Some(score) = best_score
            && self.best_score.is_none_or(|current| score > current)
        {
            self.best_score = Some(score);
        }
        if self.checked >= PARALLEL_PROGRESS_BATCH {
            self.flush(false)?;
        }
        Ok(())
    }

    #[cfg(test)]
    fn is_cancelled(&self) -> bool {
        self.shared.is_cancelled()
    }

    fn poll(&mut self) -> Result<(), String> {
        if self.shared.is_cancelled() {
            return Err("cancelled".to_string());
        }
        if self.shared.progress_every == 0 {
            return Ok(());
        }
        self.poll_count = self.poll_count.saturating_add(1);
        if self.poll_count < PROGRESS_POLL_BATCH {
            return Ok(());
        }
        self.poll_count = 0;
        self.flush(false)?;
        self.shared.emit_if_due(false, true)
    }

    fn finish(&mut self) -> Result<(), String> {
        self.flush(false)
    }
}

#[cfg(test)]
mod parallel_progress_tests {
    use super::*;

    #[test]
    fn callback_reads_progress_after_waiting_for_previous_delivery() {
        let delivered = Arc::new(AtomicU64::new(0));
        let observed = Arc::clone(&delivered);
        let progress = Arc::new(ParallelSearchProgress::new(2, 1, move |snapshot| {
            observed.store(snapshot.checked, Ordering::Relaxed);
            true
        }));
        // Hold the callback while a worker prepares a notification. More work
        // finishes before delivery, so that notification must not be obsolete.
        let delivery = progress.callback.lock().unwrap();
        let worker_progress = Arc::clone(&progress);
        let worker = std::thread::spawn(move || worker_progress.record(1, 1, None, true));
        let deadline = Instant::now() + Duration::from_secs(5);
        while progress.last_emit.lock().unwrap().last_checked != 1 {
            assert!(Instant::now() < deadline, "worker did not reach delivery");
            std::thread::yield_now();
        }
        progress.checked.fetch_add(1, Ordering::Relaxed);
        progress.eligible.fetch_add(1, Ordering::Relaxed);
        drop(delivery);
        worker.join().unwrap().unwrap();
        assert_eq!(delivered.load(Ordering::Relaxed), 2);
    }
}
