# Blackmagic DeckLink SDK headers

Interface files of the Blackmagic DeckLink SDK 12.0, used to build the
DeckLink shim (`../decklink/shim.cpp`, see `../build.rs`):

- `linux/`: C++ headers, `LinuxCOM.h`, and `DeckLinkAPIDispatch.cpp`, which
  loads `libDeckLinkAPI.so` with `dlopen` at run time.
- `mac/`: C++ headers and `DeckLinkAPIDispatch.cpp`, which loads
  `/Library/Frameworks/DeckLinkAPI.framework` at run time.
- `win/`: the IDL files. The build runs MIDL (Windows SDK) on
  `DeckLinkAPI.idl` to generate `DeckLinkAPI.h` and `DeckLinkAPI_i.c`; the
  runtime is reached through COM (`CoCreateInstance(CLSID_CDeckLinkIterator)`).

They are copied unmodified from the OBS Studio repository
(`plugins/decklink/{linux,mac,win}/decklink-sdk/`,
https://github.com/obsproject/obs-studio), which vendors them from the SDK.
Only the files needed by the shim are kept.

## Licence

Copyright (c) Blackmagic Design. These interface files are distributed by
Blackmagic Design under the permissive licence reproduced at the top of each
file (a Boost-style licence: use, reproduction, modification and
redistribution are allowed provided the copyright notice and licence text are
kept in source copies). Each file keeps its licence header.

## Runtime

The DeckLink driver and runtime library (`libDeckLinkAPI.so`,
`DeckLinkAPI.framework`, `DeckLinkAPI64.dll`) are part of Blackmagic Desktop
Video, installed separately on the machines that have DeckLink cards. They
are not shipped with this server; without them the server starts normally
and reports that Desktop Video is not installed.
