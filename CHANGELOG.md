# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog][keep-a-changelog-site],
and this project adheres to [Semantic Versioning][semver-site].

## [0.1.0] - 2026-08-24
### Added
- Disc drive routines.
  + Drive capabilities.
  + Hardware identifiers.
- Exposure of libcdio C's logs via tracing.
- ISO 9660 routines including XA, Rock Ridge, Joliet.
  + Read entry.
  + Metadata such a file name, timestamp, etc.
- SCSI MMC routines.
  + `GET CONFIG`
  + `GET EVENT STATUS`
  + `INQUIRY`
  + `PREVENT ALLOW MEDIUM REMOVAL`
  + `READ CD`
  + `READ DISC INFO`
  + `READ SUBCHANNEL`
  + `READ TOC`
  + `SET CD SPEED`
  + `START STOP UNIT`
  + `TEST UNIT READY`
- UDF filesystem routines.
  + Read entry.
  + Metadata such as file name timestamp, etc.

[0.1.0]: https://github.com/libcdio/libcdio-rs/releases/tag/v0.1.0
[keep-a-changelog-site]: https://keepachangelog.com/en/2.0.0/
[semver-site]: https://semver.org/spec/v2.0.0.html
