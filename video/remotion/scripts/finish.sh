#!/bin/bash
# Joins the rendered scenes into the picture lock, adding the film grain here (ffmpeg's temporal
# noise costs nothing; in the browser every full-screen layer cost half a second a frame), and a
# second copy with the captions burned in, in the site's typeface.
set -e
OUT=/media/dflame/UNIQ/arbit/video/out
DIR=$OUT/scenes
FONTS=${FONTS:-/media/dflame/UNIQ/arbit/video/fonts}
SRT=$(cd "$(dirname "$0")/../../docs" && pwd)/captions.srt
for id in S01 S02 S03 S04 S05 S06 S07 S08 S09 S10; do [ -f $DIR/$id.mp4 ] || { echo "missing $id"; exit 1; }; done
printf "file '%s'\n" $DIR/S0{1..9}.mp4 $DIR/S10.mp4 > $DIR/list.txt
GRAIN="noise=alls=5:allf=t"
ENC="-c:v libx264 -preset slow -crf 15 -pix_fmt yuv420p -colorspace bt709 -color_primaries bt709 -color_trc bt709 -movflags +faststart -an"
nice -n 10 ffmpeg -loglevel error -y -f concat -safe 0 -i $DIR/list.txt -vf "$GRAIN" $ENC $OUT/picture-lock.mp4
STYLE="FontName=Atkinson Hyperlegible Next SemiBold,Bold=0,FontSize=11,PrimaryColour=&H00FFFFFF,OutlineColour=&H301C0E08,BorderStyle=3,Outline=7,Shadow=0,MarginV=34,Alignment=2"
nice -n 10 ffmpeg -loglevel error -y -i $OUT/picture-lock.mp4 -vf "subtitles=$SRT:fontsdir=$FONTS:force_style='$STYLE'" $ENC $OUT/picture-lock-captions.mp4
cp $SRT $OUT/captions.srt
ls -la $OUT/*.mp4 $OUT/captions.srt
