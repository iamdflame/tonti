import { Config } from '@remotion/cli/config';

// The dusk shader needs WebGL; there is no GPU on the build machine, so ANGLE runs on SwiftShader.
Config.setChromiumOpenGlRenderer('swangle');
Config.setVideoImageFormat('jpeg');
Config.setJpegQuality(95);
Config.setConcurrency(2); // the build machine reboots under heavy parallel load
Config.setCodec('h264');
Config.setCrf(14);
Config.setPixelFormat('yuv420p');
Config.setColorSpace('bt709');
