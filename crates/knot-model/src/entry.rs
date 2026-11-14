//! Platform entry requests: the resource a launch or a platform open event asks for.

use std::path::{Path, PathBuf};

use url::Url;

use crate::{filesystem::normalize_file_uri, resource::ResourceUri};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OpenRequest {
    uri: ResourceUri,
}

impl OpenRequest {
    pub fn from_path(path: &Path, current_dir: &Path) -> Result<Self, String> {
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            current_dir.join(path)
        };
        let url = Url::from_file_path(&path).map_err(|_| {
            format!(
                "path cannot be represented as a file URI: {}",
                path.display()
            )
        })?;
        Self::from_uri(ResourceUri::from_url(url))
    }

    pub fn from_url(url: &str) -> Result<Self, String> {
        let uri = ResourceUri::parse(url).map_err(|error| error.to_string())?;
        Self::from_uri(uri)
    }

    fn from_uri(uri: ResourceUri) -> Result<Self, String> {
        normalize_file_uri(uri)
            .map(|uri| Self { uri })
            .map_err(|error| error.to_string())
    }

    pub fn from_uri_for_product(uri: ResourceUri) -> Self {
        Self { uri }
    }

    pub fn uri(&self) -> &ResourceUri {
        &self.uri
    }

    pub fn title(&self) -> String {
        self.uri
            .as_url()
            .path_segments()
            .and_then(|mut segments| segments.rfind(|segment| !segment.is_empty()))
            .map(percent_encoding::percent_decode_str)
            .and_then(|name| name.decode_utf8().ok())
            .filter(|name| !name.is_empty())
            .map(|name| name.into_owned())
            .unwrap_or_else(|| self.uri.to_string())
    }
}

/// The resources one launch asks for: one command-line invocation or one
/// platform open event.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LaunchBatch {
    requests: Vec<OpenRequest>,
}

impl LaunchBatch {
    pub fn new(requests: Vec<OpenRequest>) -> Self {
        Self { requests }
    }

    pub fn is_empty(&self) -> bool {
        self.requests.is_empty()
    }

    pub fn requests(&self) -> &[OpenRequest] {
        &self.requests
    }

    pub fn into_requests(self) -> Vec<OpenRequest> {
        self.requests
    }
}

impl From<OpenRequest> for LaunchBatch {
    fn from(request: OpenRequest) -> Self {
        Self::new(vec![request])
    }
}

/// What the process arguments ask the executable to do.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LaunchCommand {
    Version,
    Help,
    /// Start or reach the product; the batch is empty without paths.
    Open(LaunchBatch),
}

pub const USAGE: &str = "usage: knot [--version] [--help] [path ...]";

/// Parse the process arguments. Every path resolves against `current_dir`;
/// `--` ends the options.
pub fn parse_launch_arguments(
    arguments: impl IntoIterator<Item = String>,
    current_dir: &Path,
) -> Result<LaunchCommand, String> {
    let mut requests = Vec::new();
    let mut options_ended = false;
    for argument in arguments.into_iter().skip(1) {
        if !options_ended && argument.starts_with('-') && argument != "-" {
            match argument.as_str() {
                "--version" => return Ok(LaunchCommand::Version),
                "--help" => return Ok(LaunchCommand::Help),
                "--" => options_ended = true,
                _ => return Err(format!("unknown option {argument}\n{USAGE}")),
            }
            continue;
        }
        requests.push(OpenRequest::from_path(
            &PathBuf::from(argument),
            current_dir,
        )?);
    }
    Ok(LaunchCommand::Open(LaunchBatch::new(requests)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(arguments: &[&str], directory: &Path) -> Result<LaunchCommand, String> {
        parse_launch_arguments(arguments.iter().map(|a| a.to_string()), directory)
    }

    #[test]
    fn version_and_help_are_commands_in_option_position() {
        let directory = tempfile::tempdir().unwrap();
        let tmp = directory.path();
        assert_eq!(
            open(&["knot", "--version"], tmp),
            Ok(LaunchCommand::Version)
        );
        assert_eq!(open(&["knot", "a", "--help"], tmp), Ok(LaunchCommand::Help));
    }

    #[test]
    fn launch_defaults_to_an_empty_batch_without_paths() {
        assert_eq!(
            open(&["knot"], Path::new("/tmp")),
            Ok(LaunchCommand::Open(LaunchBatch::default()))
        );
    }

    #[test]
    fn unknown_options_are_usage_errors_but_paths_after_double_dash_are_not() {
        let directory = tempfile::tempdir().unwrap();
        let error = open(&["knot", "--bogus"], directory.path()).unwrap_err();
        assert!(error.contains("--bogus") && error.contains(USAGE));
        let Ok(LaunchCommand::Open(batch)) = open(&["knot", "--", "--odd"], directory.path())
        else {
            panic!("expected a batch");
        };
        assert_eq!(batch.requests()[0].title(), "--odd");
    }

    #[test]
    fn launch_accepts_many_paths_and_normalizes_them_at_the_boundary() {
        let directory = tempfile::tempdir().unwrap();
        let Ok(LaunchCommand::Open(batch)) = open(
            &["knot", "project/../notes ü.md", "other.txt"],
            directory.path(),
        ) else {
            panic!("expected a batch");
        };

        assert_eq!(batch.requests().len(), 2);
        assert_eq!(
            batch.requests()[0].uri().to_string(),
            Url::from_file_path(directory.path().join("notes ü.md"))
                .unwrap()
                .to_string()
        );
        assert_eq!(batch.requests()[0].title(), "notes ü.md");
        assert_eq!(batch.requests()[1].title(), "other.txt");
    }

    #[test]
    fn macos_open_urls_enter_as_the_same_normalized_request() {
        let request = OpenRequest::from_url("file:///tmp/project/../notes%20%C3%BC.md").unwrap();

        assert_eq!(request.uri().to_string(), "file:///tmp/notes%20%C3%BC.md");
        assert_eq!(request.title(), "notes ü.md");
        assert!(OpenRequest::from_url("https://example.com/file.txt").is_err());
    }
}
