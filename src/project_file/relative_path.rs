// SPDX-FileCopyrightText: 2026 John Dorsey
// SPDX-License-Identifier: GPL-3.0-or-later

//! Working out the path from one folder to a file, for the "relative path" backup link.

use std::path::{Component, Path, PathBuf};

/// The path that leads from the folder `base_directory` to `target`, such as
/// `../videos/holiday.mp4`.
///
/// Returns `None` when there is no such path: either argument is not absolute, or they are on
/// different drives (Windows), so no sequence of `..` can get from one to the other.
pub fn relative_path_between(base_directory: &Path, target: &Path) -> Option<PathBuf> {
    if !base_directory.is_absolute() || !target.is_absolute() {
        return None;
    }
    let base_components: Vec<Component<'_>> = base_directory.components().collect();
    let target_components: Vec<Component<'_>> = target.components().collect();

    let shared_component_count = base_components
        .iter()
        .zip(&target_components)
        .take_while(|(base_component, target_component)| base_component == target_component)
        .count();

    // The first component is the drive (Windows) and/or the root. If even that differs, the two
    // paths are on different drives.
    let root_component_count = base_components
        .iter()
        .take_while(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        .count();
    if shared_component_count < root_component_count {
        return None;
    }

    let mut relative_path = PathBuf::new();
    for _ in shared_component_count..base_components.len() {
        relative_path.push("..");
    }
    for component in &target_components[shared_component_count..] {
        relative_path.push(component.as_os_str());
    }
    if relative_path.as_os_str().is_empty() {
        relative_path.push(".");
    }
    Some(relative_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn absolute(path: &str) -> PathBuf {
        PathBuf::from(path)
    }

    #[cfg(windows)]
    fn absolute(path: &str) -> PathBuf {
        PathBuf::from(format!("C:{}", path.replace('/', "\\")))
    }

    fn relative(base: &str, target: &str) -> Option<PathBuf> {
        relative_path_between(&absolute(base), &absolute(target))
    }

    #[test]
    fn a_file_in_the_same_folder_is_just_its_name() {
        assert_eq!(
            relative("/projects/summer", "/projects/summer/holiday.mp4"),
            Some(PathBuf::from("holiday.mp4"))
        );
    }

    #[test]
    fn a_file_in_a_subfolder_goes_down() {
        assert_eq!(
            relative("/projects", "/projects/footage/day1/holiday.mp4"),
            Some(PathBuf::from("footage").join("day1").join("holiday.mp4"))
        );
    }

    #[test]
    fn a_file_in_a_sibling_folder_goes_up_then_down() {
        assert_eq!(
            relative("/projects/summer", "/videos/holiday.mp4"),
            Some(PathBuf::from("..").join("..").join("videos").join("holiday.mp4"))
        );
        assert_eq!(
            relative("/data/projects", "/data/videos/holiday.mp4"),
            Some(PathBuf::from("..").join("videos").join("holiday.mp4"))
        );
    }

    #[test]
    fn a_relative_path_resolved_against_its_base_leads_back_to_the_target() {
        let base = absolute("/data/projects/summer");
        let target = absolute("/data/videos/2026/holiday.mp4");
        let relative_path = relative_path_between(&base, &target).unwrap();
        let resolved = base.join(relative_path);
        // Lexically normalise the `..` components, as a file system would.
        let mut normalised = PathBuf::new();
        for component in resolved.components() {
            match component {
                Component::ParentDir => {
                    normalised.pop();
                }
                other => normalised.push(other.as_os_str()),
            }
        }
        assert_eq!(normalised, target);
    }

    #[test]
    fn paths_that_are_not_absolute_have_no_relative_path() {
        assert_eq!(
            relative_path_between(Path::new("projects"), &absolute("/videos/a.mp4")),
            None
        );
        assert_eq!(
            relative_path_between(&absolute("/projects"), Path::new("videos/a.mp4")),
            None
        );
    }

    #[cfg(windows)]
    #[test]
    fn files_on_different_drives_have_no_relative_path() {
        assert_eq!(
            relative_path_between(Path::new("C:\\projects"), Path::new("D:\\videos\\a.mp4")),
            None
        );
    }
}
