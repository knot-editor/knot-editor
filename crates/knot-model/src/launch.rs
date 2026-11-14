//! Where the paths of one launch batch open: in the window that already holds
//! their project, else in a new window.

use crate::{entry::OpenRequest, resource::ResourceUri};

/// A request with the kind of resource it names.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacedRequest {
    pub request: OpenRequest,
    pub directory: bool,
}

/// An open window and its workspace roots.
#[derive(Clone, Debug)]
pub struct PlacementWindow<W> {
    pub window: W,
    pub roots: Vec<ResourceUri>,
}

/// A window of a placement: one that is open, or the `n`th of
/// [`Placement::new_windows`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Destination<W> {
    Existing(W),
    New(usize),
}

/// What a batch does, as data.
#[derive(Debug, Eq, PartialEq)]
pub struct Placement<W> {
    /// The windows to create, in order. One opens a folder as its workspace;
    /// the one without a folder takes the files no window held.
    pub new_windows: Vec<Option<OpenRequest>>,
    /// The files to open, each in a window.
    pub files: Vec<(Destination<W>, OpenRequest)>,
    /// The window to focus: the last one created, otherwise the first one the
    /// batch reached. `None` for an empty batch.
    pub focus: Option<Destination<W>>,
}

/// Place a batch. `windows` are the open windows, most recently focused first.
///
/// Folders go first: a window that has the folder as a root is reused, else a
/// new window opens it. Each file goes to the most recently focused window
/// whose roots contain it, counting the windows folders created; the files
/// left over share one new window.
pub fn place<W: Clone + PartialEq>(
    windows: &[PlacementWindow<W>],
    items: &[PlacedRequest],
) -> Placement<W> {
    let mut new_windows: Vec<Option<OpenRequest>> = Vec::new();
    let mut files = Vec::new();
    let mut reached = Vec::new();
    let mut reach = |destination: Destination<W>| {
        if !reached.contains(&destination) {
            reached.push(destination);
        }
    };
    for item in items.iter().filter(|item| item.directory) {
        let uri = item.request.uri();
        let existing = windows
            .iter()
            .find(|window| window.roots.contains(uri))
            .map(|window| Destination::Existing(window.window.clone()));
        let created = new_windows
            .iter()
            .position(|folder| folder.as_ref().is_some_and(|folder| folder.uri() == uri))
            .map(Destination::New);
        reach(existing.or(created).unwrap_or_else(|| {
            new_windows.push(Some(item.request.clone()));
            Destination::New(new_windows.len() - 1)
        }));
    }
    let mut leftover = None;
    for item in items.iter().filter(|item| !item.directory) {
        let uri = item.request.uri();
        // The windows this batch created are the most recent.
        let created = new_windows
            .iter()
            .enumerate()
            .rev()
            .find(|(_, folder)| {
                folder
                    .as_ref()
                    .is_some_and(|folder| uri.is_within(folder.uri()))
            })
            .map(|(index, _)| Destination::New(index));
        let existing = || {
            windows
                .iter()
                .find(|window| window.roots.iter().any(|root| uri.is_within(root)))
                .map(|window| Destination::Existing(window.window.clone()))
        };
        let destination = created.or_else(existing).unwrap_or_else(|| {
            let index = *leftover.get_or_insert_with(|| {
                new_windows.push(None);
                new_windows.len() - 1
            });
            Destination::New(index)
        });
        reach(destination.clone());
        files.push((destination, item.request.clone()));
    }
    let focus = if new_windows.is_empty() {
        reached.first().cloned()
    } else {
        Some(Destination::New(new_windows.len() - 1))
    };
    Placement {
        new_windows,
        files,
        focus,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(path: &str) -> OpenRequest {
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        OpenRequest::from_path(std::path::Path::new(path), std::path::Path::new(root)).unwrap()
    }

    fn file(path: &str) -> PlacedRequest {
        PlacedRequest {
            request: request(path),
            directory: false,
        }
    }

    fn folder(path: &str) -> PlacedRequest {
        PlacedRequest {
            request: request(path),
            directory: true,
        }
    }

    fn window(window: u32, roots: &[&str]) -> PlacementWindow<u32> {
        PlacementWindow {
            window,
            roots: roots
                .iter()
                .map(|root| request(root).uri().clone())
                .collect(),
        }
    }

    #[test]
    fn a_folder_already_open_is_focused() {
        let windows = [window(1, &["/a"]), window(2, &["/b"])];
        let placement = place(&windows, &[folder("/b")]);
        assert!(placement.new_windows.is_empty());
        assert!(placement.files.is_empty());
        assert_eq!(placement.focus, Some(Destination::Existing(2)));
    }

    #[test]
    fn a_folder_not_open_gets_a_new_window_once() {
        let placement = place(&[window(1, &["/a"])], &[folder("/b"), folder("/b")]);
        assert_eq!(placement.new_windows, vec![Some(request("/b"))]);
        assert_eq!(placement.focus, Some(Destination::New(0)));
    }

    #[test]
    fn a_file_goes_to_the_window_whose_root_contains_it() {
        let windows = [window(1, &["/a"]), window(2, &["/b"])];
        let placement = place(&windows, &[file("/b/src/x.rs")]);
        assert!(placement.new_windows.is_empty());
        assert_eq!(
            placement.files,
            vec![(Destination::Existing(2), request("/b/src/x.rs"))]
        );
        assert_eq!(placement.focus, Some(Destination::Existing(2)));
    }

    #[test]
    fn a_file_goes_to_a_window_the_batch_created() {
        let placement = place(&[window(1, &["/a"])], &[file("/b/x.rs"), folder("/b")]);
        assert_eq!(placement.new_windows, vec![Some(request("/b"))]);
        assert_eq!(
            placement.files,
            vec![(Destination::New(0), request("/b/x.rs"))]
        );
        assert_eq!(placement.focus, Some(Destination::New(0)));
    }

    #[test]
    fn several_matches_go_to_the_most_recently_focused() {
        let windows = [window(2, &["/a/b"]), window(1, &["/a"])];
        let placement = place(&windows, &[file("/a/b/x.rs")]);
        assert_eq!(
            placement.files,
            vec![(Destination::Existing(2), request("/a/b/x.rs"))]
        );
    }

    #[test]
    fn leftover_files_share_one_new_window() {
        let windows = [window(1, &["/a"])];
        let placement = place(
            &windows,
            &[file("/n/1.md"), file("/a/x.rs"), file("/n/2.md")],
        );
        assert_eq!(placement.new_windows, vec![None]);
        assert_eq!(
            placement.files,
            vec![
                (Destination::New(0), request("/n/1.md")),
                (Destination::Existing(1), request("/a/x.rs")),
                (Destination::New(0), request("/n/2.md")),
            ]
        );
        assert_eq!(placement.focus, Some(Destination::New(0)));
    }

    #[test]
    fn without_a_created_window_the_first_file_window_is_focused() {
        let windows = [window(1, &["/a"]), window(2, &["/b"])];
        let placement = place(&windows, &[file("/b/x.rs"), file("/a/y.rs")]);
        assert_eq!(placement.focus, Some(Destination::Existing(2)));
    }

    #[test]
    fn an_empty_batch_places_nothing() {
        let placement = place::<u32>(&[], &[]);
        assert_eq!(placement.focus, None);
        assert!(placement.new_windows.is_empty());
    }
}
