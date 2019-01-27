// Copyright 2019 The Chromium OS Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

use sample::{self, Frame, FromSample, Sample, Signal};

pub fn process<F, T>(signals: &mut Vec<Box<dyn Signal<Frame = F>>>, samples_out: &mut [T])
where
    F: Frame<Sample = f32>,
    T: Frame<NumChannels = <F as Frame>::NumChannels>,
    T::Sample: FromSample<f32>,
{
    let mix = SignalMixer { signals };
    for (s, out) in mix
        .dc_block(0.7)
        .scale_amp(0.5)
        .take(48000)
        .map(|frame| frame.map(|sample| sample.to_sample::<T::Sample>()))
        .zip(samples_out.iter_mut())
    {
        *out = s;
    }
}

/// Provides an iterator that offsets the amplitude of every channel in each frame of the
/// signal by some sample value and yields the resulting frames.
//#[derive(Clone)]
pub struct SignalMixer<'a, F>
where
    F: Frame,
{
    signals: &'a mut Vec<Box<dyn Signal<Frame = F>>>,
}

impl<'a, F> Signal for SignalMixer<'a, F>
where
    F: Frame,
{
    type Frame = F;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        let mut frame = Self::Frame::equilibrium();
        for s in self.signals.iter_mut() {
            frame = frame.add_amp(s.next().to_signed_frame());
        }
        frame
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signals.iter().any(|s| s.is_exhausted())
    }
}

/// An `Iterator` that blocks any dc offset present in the signal.
pub struct DCFiltered<S>
where
    S: Signal,
{
    signal: S,
    r: <S::Frame as Frame>::Sample, // coefficient of the filter
    // Filter state.
    x_prev: <S::Frame as Frame>::Signed,
    y_prev: <S::Frame as Frame>::Signed,
}

impl<S> Signal for DCFiltered<S>
where
    S: Signal,
{
    type Frame = S::Frame;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        let x = self.signal.next().to_signed_frame();
        // d = x - x_prev + r * y_prev;
        let neg_prev = self.x_prev.scale_amp((-1.0).to_sample());
        let d = x.add_amp(neg_prev).add_amp(
            self.y_prev
                .scale_amp(self.r.to_signed_sample().to_float_sample()),
        );
        self.y_prev = d;
        self.x_prev = x;
        Self::Frame::equilibrium().add_amp(d)
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signal.is_exhausted()
    }
}

/// An 'Iterator' that mutes the samples from its signal.
pub struct Muted<S>
where
    S: Signal,
{
    signal: S,
}

impl<S> Signal for Muted<S>
where
    S: Signal,
{
    type Frame = S::Frame;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        Self::Frame::equilibrium()
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signal.is_exhausted()
    }
}

/// An 'Iterator' that filters the samples from its signal.
/// The previous two inputs are stored in x1 and x2, and the previous two outputs are
/// stored in y1 and y2.
///
/// We use f64 during the coefficients calculation for better accurary, but
/// f32 is used during the actual filtering for faster computation.
pub struct BiQuad<S>
where
    S: Signal,
{
    signal: S,
    b0: <S::Frame as Frame>::Sample,
    b1: <S::Frame as Frame>::Sample,
    b2: <S::Frame as Frame>::Sample,
    neg_a1: <S::Frame as Frame>::Sample,
    neg_a2: <S::Frame as Frame>::Sample,
    x1: S::Frame,
    x2: S::Frame,
    y1: S::Frame,
    y2: S::Frame,
}

impl<S> BiQuad<S>
where
    S: Signal,
    <S::Frame as Frame>::Sample: FromSample<f64>,
{
    pub fn new(signal: S, b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        let a0_inv: f64 = 1.0 / a0;
        Self {
            signal,
            b0: (b0 * a0_inv).to_sample(),
            b1: (b1 * a0_inv).to_sample(),
            b2: (b2 * a0_inv).to_sample(),
            neg_a1: (a1 * a0_inv * -1.0).to_sample(),
            neg_a2: (a2 * a0_inv * -1.0).to_sample(),
            x1: S::Frame::equilibrium(),
            x2: S::Frame::equilibrium(),
            y1: S::Frame::equilibrium(),
            y2: S::Frame::equilibrium(),
        }
    }
}

impl<S> Signal for BiQuad<S>
where
    S: Signal,
{
    type Frame = S::Frame;

    #[inline]
    fn next(&mut self) -> Self::Frame {
        // The transfer function H(z) is:
        // (b0 + b1 * z^(-1) + b2 * z^(-2)) / (1 + a1 * z^(-1) + a2 * z^(-2)).

        let x1_b1 = self
            .x1
            .scale_amp(self.b1.to_float_sample())
            .to_signed_frame();
        let x2_b2 = self
            .x2
            .scale_amp(self.b2.to_float_sample())
            .to_signed_frame();
        let y1_neg_a1 = self
            .y1
            .scale_amp(self.neg_a1.to_float_sample())
            .to_signed_frame();
        let y2_neg_a2 = self
            .y2
            .scale_amp(self.neg_a2.to_float_sample())
            .to_signed_frame();
        self.signal
            .next()
            .scale_amp(self.b0.to_float_sample())
            .add_amp(x1_b1)
            .add_amp(x2_b2)
            .add_amp(y1_neg_a1)
            .add_amp(y2_neg_a2)
    }

    #[inline]
    fn is_exhausted(&self) -> bool {
        self.signal.is_exhausted()
    }
}

/// Addition to `Signal` that adds some basic processing functions.
pub trait DspProcessable {
    fn mute(self) -> Muted<Self>
    where
        Self: Sized + Signal,
    {
        Muted { signal: self }
    }

    fn dc_block<S>(self, coefficient: S) -> DCFiltered<Self>
    where
        Self: Sized + Signal,
        S: Sample,
        Self::Frame: Frame<Sample = S>,
    {
        DCFiltered {
            signal: self,
            r: coefficient,
            y_prev: Self::Frame::equilibrium().to_signed_frame(),
            x_prev: Self::Frame::equilibrium().to_signed_frame(),
        }
    }
}

impl<S: Signal> DspProcessable for S {}

#[cfg(test)]
mod tests {
    use super::*;

    use sample::{signal, Frame, Sample, Signal};

    #[test]
    fn single_stream() {
        let mut signals: Vec<Box<dyn Signal<Frame = [f32; 1]>>> = vec![
            Box::new(
                signal::rate(48000.0)
                    .const_hz(440.0)
                    .sine()
                    .map(|f| f.map(|s| s.to_sample::<f32>())),
            ),
            Box::new(signal::from_iter(std::iter::repeat([0.25f32; 1]))),
        ];
        let mut out_mem = [[0i32; 1]; 48000];

        process(&mut signals, &mut out_mem);
    }
}
