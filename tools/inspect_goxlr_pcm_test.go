package main

import (
	"encoding/binary"
	"math"
	"testing"
)

func TestStereoCancellationIsVisible(t *testing.T) {
	data := make([]byte, 480*8)
	for i := 0; i < 480; i++ {
		value := float32(0.5 * math.Sin(2*math.Pi*10*float64(i)/480))
		binary.LittleEndian.PutUint32(data[i*8:], math.Float32bits(value))
		binary.LittleEndian.PutUint32(data[i*8+4:], math.Float32bits(-value))
	}
	got, err := inspectPCM(data, 2)
	if err != nil {
		t.Fatal(err)
	}
	if math.Abs(got.Channels[0].RMSDBFS+9.0309) > 0.001 || got.StereoCorrelation == nil ||
		math.Abs(*got.StereoCorrelation+1) > 1e-6 || *got.MonoMixRMSDBFS > -200 {
		t.Fatalf("Missed valid stereo signal cancelled by mono downmix: %+v", got)
	}
}

func TestRejectsMalformedPCM(t *testing.T) {
	invalid := make([]byte, 8)
	binary.LittleEndian.PutUint32(invalid, math.Float32bits(float32(math.NaN())))
	for _, data := range [][]byte{nil, {0, 0, 0}, invalid} {
		if _, err := inspectPCM(data, 2); err == nil {
			t.Fatalf("Accepted malformed PCM: %v", data)
		}
	}
}
