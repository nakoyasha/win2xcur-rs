> [!WARNING]
> This has been entirely made by Claude, and thus I do not claim any responsibility or even guarantee that this code will be maintained or even functional in a year or more. I've decided to publish it anyway because of it's usefulness (and improvement over the original) despite it being intended for personal use.

# win2xcur-rs

This is an expanded and rewritten version of the original [win2xcur](https://github.com/quantum5/win2xcur) by quantum5. It is significantly smaller and more portable than it's Python counter-part, with a release build weighing at just around ~500 kb. 

# Expanded features from original
- Proper support for pixel cursors
- Proper cursor size options (by default, up to 128)

This was mainly done so that the official [SEKAI cursors](https://colorfulstage.com/media/download/) could work as intended under Linux. Notoriously, the original win2xcur produced issues and artefacts such as needing to use a cursor with proper sizes before these as otherwise they would either be too big or too small, and even then the size varied between apps depending on whether they are X11 apps, or GTK apps for example.
