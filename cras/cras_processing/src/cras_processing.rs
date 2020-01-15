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
#[derive(Clone)]
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
#[derive(Clone)]
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
// TODO store the coefficients as floats.
#[derive(Clone)]
pub struct BiQuad<S>
where
    S: Signal,
{
    signal: S,
    b0: S::Frame,
    b1: S::Frame,
    b2: S::Frame,
    inv_a1: S::Frame,
    inv_a2: S::Frame,
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
    pub fn new(
        signal: S,
        b0: S::Frame,
        b1: S::Frame,
        b2: S::Frame,
        a0: S::Frame,
        a1: S::Frame,
        a2: S::Frame,
    ) -> Self {
        Self {
            signal,
            b0: b0.div_amp(a0.to_float_frame()),
            b1: b1.mul_amp(a0.to_float_frame()),
            b2: b2.div_amp(a0.to_float_frame()),
            inv_a1: a1.div_amp(a0.to_float_frame()),
            inv_a2: a2.div_amp(a0.to_float_frame()),
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

        let x1_b1 = self.x1.mul_amp(self.b1.to_float_frame());
        let x2_b2 = self.x2.mul_amp(self.b2.to_float_frame());
        let y1_a1 = self.y1.mul_amp(self.inv_a1.to_float_frame());
        let y2_a2 = self.y2.mul_amp(self.inv_a2.to_float_frame());
        self.signal
            .next()
            .mul_amp(self.b0.to_float_frame())
            .add_amp(x1_b1.to_signed_frame())
            .add_amp(x2_b2.to_signed_frame())
            .sub_amp(y1_a1.to_signed_frame())
            .sub_amp(y2_a2.to_signed_frame())
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

    fn biquad(self, b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        // The following `unwrap` calls cannot fail because `from_samples` only returns `None` when
        // insufficient samples are provided, `repeat` provides infinite samples.
        BiQuad::new(
            self,
            Self::Frame::from_samples(&mut std::iter::repeat(b0.to_sample())).unwrap(),
            Self::Frame::from_samples(&mut std::iter::repeat(b1.to_sample())).unwrap(),
            Self::Frame::from_samples(&mut std::iter::repeat(b2.to_sample())).unwrap(),
            Self::Frame::from_samples(&mut std::iter::repeat(a0.to_sample())).unwrap(),
            Self::Frame::from_samples(&mut std::iter::repeat(a1.to_sample())).unwrap(),
            Self::Frame::from_samples(&mut std::iter::repeat(a2.to_sample())).unwrap(),
        )
    }

    /// Passes the signal through lowpass filter. The frequency must be between 0.0 and 1.0.
    fn low_pass<F>(self, cutoff_freq: F, resonance: F) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
        F: Frame<Sample = f64, NumChannels = <<Self as Signal>::Frame as Frame>::NumChannels>,
    {
        fn lowpass_params(cutoff: f64, resonance: f64) -> (f64, f64, f64) {
            // Compute biquad coefficients for lowpass filter
            let resonance = if resonance < 0.0 { 0.0 } else { resonance }; // can't go negative
            let g: f64 = 10.0_f64.powf(0.05 * resonance);
            let d: f64 = ((4.0 - (16.0 - 16.0 / (g * g)).sqrt()) / 2.0).sqrt();

            let theta: f64 = std::f64::consts::PI * cutoff;
            let sn: f64 = 0.5 * d * theta.sin();
            let beta: f64 = 0.5 * (1.0 - sn) / (1.0 + sn);
            let gamma: f64 = (0.5 + beta) * theta.cos();
            let alpha: f64 = 0.25 * (0.5 + beta - gamma);
            (alpha, beta, gamma)
        }

        let mut b0 = Self::Frame::equilibrium();
        let mut b1 = Self::Frame::equilibrium();
        let mut b2 = Self::Frame::equilibrium();
        let mut a0 = Self::Frame::equilibrium();
        let mut a1 = Self::Frame::equilibrium();
        let mut a2 = Self::Frame::equilibrium();
        for (i, (c, r)) in cutoff_freq.channels().zip(resonance.channels()).enumerate() {
            let coeffs = match c {
                _f if _f >= 1.0 => {
                    // When cutoff is 1, the z-transform is 1.
                    (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
                }
                _f if _f <= 0.0 => {
                    // When cutoff is zero, nothing gets through the filter, so set
                    // coefficients up correctly.
                    (0.0, 0.0, 0.0, 1.0, 0.0, 0.0)
                }
                f => {
                    let (alpha, beta, gamma) = lowpass_params(f, r);
                    (
                        2.0 * alpha,       // b0
                        2.0 * 2.0 * alpha, // b1
                        2.0 * alpha,       // b2
                        1.0,               // a0
                        2.0 * -gamma,      // a1
                        2.0 * beta,        // a2
                    )
                }
            };
            b0.set_channel(i, coeffs.0.to_sample());
            b1.set_channel(i, coeffs.1.to_sample());
            b2.set_channel(i, coeffs.2.to_sample());
            a0.set_channel(i, coeffs.3.to_sample());
            a1.set_channel(i, coeffs.4.to_sample());
            a2.set_channel(i, coeffs.5.to_sample());
        }

        BiQuad::new(self, b0, b1, b2, a0, a1, a2)
    }

    /// Passes the signal through highpass filter. The frequency must be between 0.0 and 1.0.
    fn high_pass(self, cutoff_freq: f64, resonance: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        match cutoff_freq {
            _f if _f >= 1.0 => {
                // When cutoff is one, nothing gets through the filter, so set
                // coefficients up correctly.
                self.biquad(0.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            _f if _f <= 0.0 => {
                // When cutoff is zero, the z-transform is 1.
                self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            f => {
                // Compute biquad coefficients for highpass filter
                let resonance = if resonance < 0.0 { 0.0 } else { resonance }; // can't go negative
                let g: f64 = 10.0_f64.powf(0.05 * resonance);
                let d: f64 = ((4.0 - (16.0 - 16.0 / (g * g)).sqrt()) / 2.0).sqrt();

                let theta: f64 = std::f64::consts::PI * f;
                let sn: f64 = 0.5 * d * theta.sin();
                let beta: f64 = 0.5 * (1.0 - sn) / (1.0 + sn);
                let gamma: f64 = (0.5 + beta) * theta.cos();
                let alpha: f64 = 0.25 * (0.5 + beta + gamma);

                let b0: f64 = 2.0 * alpha;
                let b1: f64 = 2.0 * -2.0 * alpha;
                let b2: f64 = 2.0 * alpha;
                let a1: f64 = 2.0 * -gamma;
                let a2: f64 = 2.0 * beta;
                self.biquad(b0, b1, b2, 1.0, a1, a2)
            }
        }
    }

    /// Passes the signal through a bandpass filter.
    fn bandpass(self, freq: f64, q: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        if q <= 0.0 {
            // When Q = 0, the formulas have problems. If we look at the z-transform, we can
            // see that the limit as Q->0 is 1, so set the filter that way.
            return self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0);
        }

        match freq {
            f if f > 0.0 && f < 1.0 => {
                let w0 = std::f64::consts::PI * f;
                let alpha = w0.sin() / (2.0 * q);
                let k = w0.cos();

                let b0 = alpha;
                let b1 = 0.0;
                let b2 = -alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * k;
                let a2 = 1.0 - alpha;

                self.biquad(b0, b1, b2, a0, a1, a2)
            }
            _ => {
                // When the cutoff is zero, the z-transform approaches 0, if Q > 0. When both Q and
                // cutoff are zero, the z-transform is pretty much undefined. What should we do in
                // this case?  For now, just make the filter 0. When the cutoff is 1, the
                // z-transform also approaches 0.
                self.biquad(0.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
        }
    }

    /// Passes the signal through a low shelf filter.
    fn low_shelf(self, freq: f64, db_gain: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        let a = 10.0_f64.powf(db_gain / 40.0);

        match freq {
            _f if _f >= 1.0 => {
                // Passes through applying a constant gain.
                self.biquad(a * a, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            _f if _f <= 0.0 => {
                // The signal in unaffected it the shelf is at zero.
                self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            f => {
                let w0 = std::f64::consts::PI * f;
                let s = 1.0; // filter slope (one is the max value)
                let alpha = 0.5 * w0.sin() * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).sqrt();
                let k = w0.cos();
                let k2 = 2.0 * a.sqrt() * alpha;
                let a_plus_one = a + 1.0;
                let a_minus_one = a - 1.0;

                let b0 = a * (a_plus_one - a_minus_one * k + k2);
                let b1 = 2.0 * a * (a_minus_one - a_plus_one * k);
                let b2 = a * (a_plus_one - a_minus_one * k - k2);
                let a0 = a_plus_one + a_minus_one * k + k2;
                let a1 = -2.0 * (a_minus_one + a_plus_one * k);
                let a2 = a_plus_one + a_minus_one * k - k2;

                self.biquad(b0, b1, b2, a0, a1, a2)
            }
        }
    }

    /// Passes the signal through a high shelf filter.
    fn high_shelf(self, freq: f64, db_gain: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        let a = 10.0_f64.powf(db_gain / 40.0);

        match freq {
            _f if _f >= 1.0 => {
                // The signal in unaffected it the shelf is past the highest frequency.
                self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            _f if _f <= 0.0 => {
                // Passes through applying a constant gain (all shelf).
                self.biquad(a * a, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            f => {
                let w0 = std::f64::consts::PI * f;
                let s = 1.0; // filter slope (one is the max value)
                let alpha = 0.5 * w0.sin() * ((a + 1.0 / a) * (1.0 / s - 1.0) + 2.0).sqrt();
                let k = w0.cos();
                let k2 = 2.0 * a.sqrt() * alpha;
                let a_plus_one = a + 1.0;
                let a_minus_one = a - 1.0;

                let b0 = a * (a_plus_one + a_minus_one * k + k2);
                let b1 = 2.0 * a * (a_minus_one + a_plus_one * k);
                let b2 = a * (a_plus_one + a_minus_one * k - k2);
                let a0 = a_plus_one - a_minus_one * k + k2;
                let a1 = -2.0 * (a_minus_one - a_plus_one * k);
                let a2 = a_plus_one - a_minus_one * k - k2;

                self.biquad(b0, b1, b2, a0, a1, a2)
            }
        }
    }

    /// Passes the signal through a peaking filter.
    fn peaking(self, freq: f64, q: f64, db_gain: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        let a = 10.0_f64.powf(db_gain / 40.0);

        match freq {
            _f if _f <= 0.0 || _f >= 1.0 => {
                // When the frequency is zero or one, the signal in unaffected.
                self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            f => {
                if q <= 0.0 {
                    // When Q = 0, the above formulas have problems. If we look at the z-transform, we can
                    // see that the limit as Q->0 is A^2, so set the filter that way.
                    return self.biquad(a * a, 0.0, 0.0, 1.0, 0.0, 0.0);
                }
                let w0 = std::f64::consts::PI * f;
                let alpha = w0.sin() / (2.0 * q);
                let k = w0.cos();

                let b0 = 1.0 + alpha * a;
                let b1 = -2.0 * k;
                let b2 = 1.0 - alpha * a;
                let a0 = 1.0 + alpha / a;
                let a1 = -2.0 * k;
                let a2 = 1.0 - alpha / a;

                self.biquad(b0, b1, b2, a0, a1, a2)
            }
        }
    }

    /// Passes the signal through a notch filter.
    fn notch(self, freq: f64, q: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        match freq {
            _f if _f <= 0.0 || _f >= 1.0 => {
                // When the frequency is zero or one, the signal in unaffected.
                self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            f => {
                if q <= 0.0 {
                    // When Q = 0, the above formulas have problems. If we look at the z-transform,
                    // we can see that the limit as Q->0 is 0, so set the filter that way.
                    return self.biquad(0.0, 0.0, 0.0, 1.0, 0.0, 0.0);
                }
                let w0 = std::f64::consts::PI * f;
                let alpha = w0.sin() / (2.0 * q);
                let k = w0.cos();

                let b0 = 1.0;
                let b1 = -2.0 * k;
                let b2 = 1.0;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * k;
                let a2 = 1.0 - alpha;

                self.biquad(b0, b1, b2, a0, a1, a2)
            }
        }
    }

    /// Passes the signal through a all pass filter.
    fn all_pass(self, freq: f64, q: f64) -> BiQuad<Self>
    where
        Self: Sized + Signal,
        <<Self as Signal>::Frame as Frame>::Sample: FromSample<f64>,
    {
        match freq {
            _f if _f <= 0.0 || _f >= 1.0 => {
                // When the frequency is zero or one, the signal in unaffected.
                self.biquad(1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
            }
            f => {
                if q <= 0.0 {
                    // When Q = 0, the above formulas have problems. If we look at the z-transform,
                    // we can see that the limit as Q->0 is -1, so set the filter that way.
                    return self.biquad(0.0, 0.0, 0.0, 1.0, 0.0, 0.0);
                }
                let w0 = std::f64::consts::PI * f;
                let alpha = w0.sin() / (2.0 * q);
                let k = w0.cos();

                let b0 = 1.0 - alpha;
                let b1 = -2.0 * k;
                let b2 = 1.0 + alpha;
                let a0 = 1.0 + alpha;
                let a1 = -2.0 * k;
                let a2 = 1.0 - alpha;

                self.biquad(b0, b1, b2, a0, a1, a2)
            }
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
