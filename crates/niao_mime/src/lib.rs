//! File-type detection by magic bytes, extension<->MIME maps.
//! (~python-magic + filetype + mimetypes subset)

mod batch;
mod categories;
mod detector;
mod error;
mod extmap;
mod guess;
mod magic;
mod parse;
mod types;

pub use batch::{parallel_detect, parallel_from_bytes, parallel_guess_types, parallel_sniff_paths};
pub use categories::{
    is_archive_mime, is_audio_mime, is_font_mime, is_image_mime, is_text_mime, is_video_mime,
    kind_name, kind_of_mime,
};
pub use detector::Detector;
pub use error::{MimeError, MimeResult};
pub use extmap::{normalize_ext, MimeRegistry};
pub use guess::{
    extension_from_path, filename_from_path, from_bytes, from_path, guess_extension_from_bytes,
    guess_mime, read_sniff_bytes, sniff_path, SniffOpts,
};
pub use magic::{
    bytes_match_mime, match_bytes, parse_hex_magic, signature_count, CustomMagic,
    BUILTIN_SIGNATURES, DEFAULT_SNIFF_BYTES, MAX_SNIFF_BYTES,
};
pub use parse::{
    is_valid_mime, kind_from_mime, mime_matches, normalize_mime, parse_mime, ParsedMime,
};
pub use types::{FileKind, GuessTypeResult, MatchSource, MimeMatch};

#[cfg(test)]
mod integration {
    use super::*;

    #[test]
    fn roundtrip_ext_mime() {
        let reg = MimeRegistry::builtin();
        let mime = reg.extension_to_mime("png", false).unwrap();
        assert_eq!(mime, "image/png");
        let exts = reg.mime_to_extensions("image/png", false);
        assert!(exts.contains(&"png".to_string()));
    }
}
