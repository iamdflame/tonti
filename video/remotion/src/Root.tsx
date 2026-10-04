import { Composition } from 'remotion';
import { ThumbA, ThumbB } from './Thumbnail';
import { Architecture, Logo, LogoMark, Proof } from './Submission';
import { Master, One, type MasterProps } from './Master';
import { SCENES, TOTAL } from './timeline';
import { FPS, H, W } from './theme';

export const Root = () => (
  <>
    <Composition id="Tonti" component={Master} durationInFrames={TOTAL} fps={FPS} width={W} height={H} defaultProps={{ captions: false } as MasterProps} />
    <Composition id="TontiCaptions" component={Master} durationInFrames={TOTAL} fps={FPS} width={W} height={H} defaultProps={{ captions: true } as MasterProps} />
    {SCENES.map((s) => (
      <Composition key={s.id} id={s.id} component={One} durationInFrames={s.frames} fps={FPS} width={W} height={H} defaultProps={{ id: s.id, captions: false } as MasterProps & { id: string }} />
    ))}
    <Composition id="ThumbA" component={ThumbA} durationInFrames={1} fps={FPS} width={1280} height={720} />
    <Composition id="ThumbB" component={ThumbB} durationInFrames={1} fps={FPS} width={1280} height={720} />
    <Composition id="Logo" component={Logo} durationInFrames={1} fps={FPS} width={1024} height={1024} />
    <Composition id="LogoMark" component={LogoMark} durationInFrames={1} fps={FPS} width={1024} height={1024} />
    <Composition id="Architecture" component={Architecture} durationInFrames={1} fps={FPS} width={1280} height={720} />
    <Composition id="Proof" component={Proof} durationInFrames={1} fps={FPS} width={1280} height={720} />
  </>
);
