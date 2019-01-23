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

/// Addition to `Signal` that adds some basic processing functions.
trait DspProcessable {
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
