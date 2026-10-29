# Language service

`xcss-language-service` provides editor queries without generation, filesystem
access, or a protocol dependency. Hosts provide normalized file identities, text,
monotonically increasing revisions, and explicit import resolutions.

`LanguageService::snapshot()` retains an immutable project view. Queries return
owned diagnostics, definitions, references, hover text, and semantic tokens. Symbol
IDs never cross this interface. All editor ranges use zero-based UTF-16 positions;
the parser and analyzer continue using original UTF-8 byte ranges.

The published `@cross-css/native` package exposes `LanguageService` with `setFile`,
`removeFile`, `setResolution`, `document`, `definition`, `references`, and `hover`.
`languageServiceCapabilities()` supplies API version 1 and the ordered token legend.
`document()` returns a project revision, structured diagnostics, and absolute token
positions; adapters encode tokens for their protocol. Hover text is plain text.

The service reports syntax and language diagnostics. Backend-specific emission
errors still belong to compilation. Lint policy, completion, rename, and code fixes
are separate capabilities. Existing `Compiler` APIs remain available.
