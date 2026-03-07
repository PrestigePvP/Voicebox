package audio

import (
	"math"
	"sync"
	"time"

	"github.com/gen2brain/malgo"
)

type Capture struct {
	ctx         *malgo.AllocatedContext
	device      *malgo.Device
	chunks      chan []byte
	buf         []byte
	mu          sync.Mutex
	chunkSize   int
	stopOnce    sync.Once
	onLevel     func(float64)
	lastLevelAt time.Time
}

func NewCapture(sampleRate, channels, chunkSize int, onLevel func(float64)) (*Capture, error) {
	ctx, err := malgo.InitContext(nil, malgo.ContextConfig{}, nil)
	if err != nil {
		return nil, err
	}

	c := &Capture{
		ctx:       ctx,
		chunks:    make(chan []byte, 64),
		chunkSize: chunkSize,
		onLevel:   onLevel,
	}

	deviceConfig := malgo.DefaultDeviceConfig(malgo.Capture)
	deviceConfig.Capture.Format = malgo.FormatS16
	deviceConfig.SampleRate = uint32(sampleRate)
	deviceConfig.Capture.Channels = uint32(channels)

	callbacks := malgo.DeviceCallbacks{
		Data: func(_, pInputSamples []byte, frameCount uint32) {
			if len(pInputSamples) == 0 {
				return
			}

			if c.onLevel != nil && time.Since(c.lastLevelAt) >= 33*time.Millisecond {
				c.lastLevelAt = time.Now()
				c.onLevel(rmsLevel(pInputSamples))
			}

			c.mu.Lock()
			defer c.mu.Unlock()
			c.buf = append(c.buf, pInputSamples...)
			for len(c.buf) >= c.chunkSize {
				chunk := make([]byte, c.chunkSize)
				copy(chunk, c.buf[:c.chunkSize])
				c.buf = c.buf[c.chunkSize:]
				select {
				case c.chunks <- chunk:
				default:
				}
			}
		},
	}

	device, err := malgo.InitDevice(ctx.Context, deviceConfig, callbacks)
	if err != nil {
		ctx.Uninit()
		ctx.Free()
		return nil, err
	}
	c.device = device

	return c, nil
}

func rmsLevel(samples []byte) float64 {
	if len(samples) < 2 {
		return 0
	}
	var sum float64
	n := len(samples) / 2
	for i := 0; i < len(samples)-1; i += 2 {
		s := int16(samples[i]) | int16(samples[i+1])<<8
		sum += float64(s) * float64(s)
	}
	return math.Sqrt(sum/float64(n)) / 32768.0
}

func (c *Capture) Start() error {
	return c.device.Start()
}

func (c *Capture) Stop() {
	c.stopOnce.Do(func() {
		c.device.Stop()

		c.mu.Lock()
		if len(c.buf) > 0 {
			remaining := make([]byte, len(c.buf))
			copy(remaining, c.buf)
			c.buf = nil
			c.mu.Unlock()
			select {
			case c.chunks <- remaining:
			default:
			}
		} else {
			c.mu.Unlock()
		}
		close(c.chunks)
	})
}

func (c *Capture) Chunks() <-chan []byte {
	return c.chunks
}

func (c *Capture) Close() {
	c.device.Uninit()
	c.ctx.Uninit()
	c.ctx.Free()
}
