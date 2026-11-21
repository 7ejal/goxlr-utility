// Inspect captured float32 little-endian PCM without mixing stereo channels.
// Run: go run tools/inspect_goxlr_pcm.go -input recording.f32 -channels 2
package main

import (
	"encoding/binary"
	"encoding/json"
	"flag"
	"fmt"
	"math"
	"os"
)

type channelStats struct {
	PeakDBFS, RMSDBFS float64
	ClippedSamples    int
}

type pcmStats struct {
	Frames            int
	Channels          []channelStats
	StereoCorrelation *float64 `json:",omitempty"`
	MonoMixRMSDBFS    *float64 `json:",omitempty"`
}

func inspectPCM(data []byte, channels int) (pcmStats, error) {
	if (channels != 1 && channels != 2) || len(data) == 0 || len(data)%(4*channels) != 0 {
		return pcmStats{}, fmt.Errorf("expected complete nonempty mono/stereo float32 frames")
	}
	r := pcmStats{Frames: len(data) / (4 * channels), Channels: make([]channelStats, channels)}
	var sums, squares [2]float64
	var product, monoSquares float64
	for frame := 0; frame < r.Frames; frame++ {
		var sample [2]float64
		for ch := 0; ch < channels; ch++ {
			offset := (frame*channels + ch) * 4
			value := float64(math.Float32frombits(binary.LittleEndian.Uint32(data[offset:])))
			if math.IsNaN(value) || math.IsInf(value, 0) {
				return pcmStats{}, fmt.Errorf("nonfinite sample at frame %d channel %d", frame, ch)
			}
			sample[ch] = value
			sums[ch] += value
			squares[ch] += value * value
			r.Channels[ch].PeakDBFS = math.Max(r.Channels[ch].PeakDBFS, math.Abs(value))
			if math.Abs(value) >= 1 {
				r.Channels[ch].ClippedSamples++
			}
		}
		product += sample[0] * sample[1]
		mono := (sample[0] + sample[1]) / 2
		monoSquares += mono * mono
	}
	n := float64(r.Frames)
	for ch := range r.Channels {
		r.Channels[ch].PeakDBFS = 20 * math.Log10(math.Max(1e-12, r.Channels[ch].PeakDBFS))
		r.Channels[ch].RMSDBFS = 10 * math.Log10(math.Max(1e-24, squares[ch]/n))
	}
	if channels == 2 {
		denominator := math.Sqrt(math.Max(0, squares[0]-sums[0]*sums[0]/n) * math.Max(0, squares[1]-sums[1]*sums[1]/n))
		if denominator > 0 {
			correlation := math.Max(-1, math.Min(1, (product-sums[0]*sums[1]/n)/denominator))
			r.StereoCorrelation = &correlation
		}
		mono := 10 * math.Log10(math.Max(1e-24, monoSquares/n))
		r.MonoMixRMSDBFS = &mono
	}
	return r, nil
}

func main() {
	input := flag.String("input", "", "captured f32le PCM file")
	channels := flag.Int("channels", 2, "channel count: 1 or 2")
	flag.Parse()
	data, err := os.ReadFile(*input)
	if err == nil {
		var result pcmStats
		result, err = inspectPCM(data, *channels)
		if err == nil {
			err = json.NewEncoder(os.Stdout).Encode(result)
		}
	}
	if err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
