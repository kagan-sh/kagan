use std::collections::BTreeMap;

use gpui_kit::{Context, FocusHandle, Window, actions};
use kg_domain::DiffFile;

use super::Kagan;

actions!(review_tree, [Up, Down, Left, Right, Home, End, Open]);

pub(in crate::kagan) const CONTEXT: &str = "ReviewTree";

const FOLDER_INSET: f32 = 8.;
const CHEVRON_WIDTH: f32 = 12.;
const ROW_GAP: f32 = 4.;
const FILE_INSET: f32 = FOLDER_INSET + CHEVRON_WIDTH + ROW_GAP;

#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<&'a str, Folder<'a>>,
    files: Vec<&'a DiffFile>,
    count: usize,
}

#[derive(Clone, Copy)]
enum TreeKey {
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    Open,
}

#[derive(Clone)]
struct TreeRow {
    path: String,
    folder: bool,
    focus: FocusHandle,
    tab: bool,
}

fn ordered_files(
    folder: &Folder<'_>,
    parent: &str,
    collapsed: &std::collections::BTreeSet<String>,
    out: &mut Vec<(String, bool)>,
) {
    for (name, child) in &folder.folders {
        let (_, path, child) = compact_folder(name, child, parent);
        out.push((path.clone(), true));
        if !collapsed.contains(&path) {
            ordered_files(child, &path, collapsed, out);
        }
    }
    out.extend(folder.files.iter().map(|file| (file.path.clone(), false)));
}

fn folder_tree<'a>(files: &[&'a DiffFile]) -> Folder<'a> {
    let mut root = Folder::default();
    for file in files {
        let mut folder = &mut root;
        folder.count += 1;
        let parts: Vec<_> = file.path.split('/').collect();
        for part in &parts[..parts.len().saturating_sub(1)] {
            folder = folder.folders.entry(part).or_default();
            folder.count += 1;
        }
        folder.files.push(file);
    }
    root
}

fn compact_folder<'a>(
    name: &str,
    child: &'a Folder<'a>,
    parent: &str,
) -> (String, String, &'a Folder<'a>) {
    let mut name = name.to_owned();
    let mut path = if parent.is_empty() {
        name.clone()
    } else {
        format!("{parent}/{name}")
    };
    let mut child = child;
    while child.files.is_empty() && child.folders.len() == 1 {
        let Some((next, nested)) = child.folders.first_key_value() else {
            break;
        };
        name.push('/');
        name.push_str(next);
        path.push('/');
        path.push_str(next);
        child = nested;
    }
    (name, path, child)
}

fn tree_order(
    root: &Folder<'_>,
    files: &[&DiffFile],
    review: &super::Review,
    window: &mut Window,
    cx: &mut Context<Kagan>,
) -> std::rc::Rc<Vec<TreeRow>> {
    let mut paths = Vec::new();
    if review.tree {
        ordered_files(root, "", &review.collapsed, &mut paths);
    } else {
        paths.extend(files.iter().map(|file| (file.path.clone(), false)));
    }
    let active = review
        .tree_focus
        .as_ref()
        .filter(|path| paths.iter().any(|(candidate, _)| candidate == *path))
        .or_else(|| {
            paths
                .iter()
                .find(|(path, _)| *path == review.path)
                .map(|(path, _)| path)
        })
        .or_else(|| paths.first().map(|(path, _)| path))
        .cloned();
    let order: std::rc::Rc<Vec<TreeRow>> = std::rc::Rc::new(
        paths
            .into_iter()
            .map(|(path, folder)| {
                let focus = super::super::tab_stop(window, cx, format!("review-tree-focus-{path}"));
                TreeRow {
                    tab: active.as_ref() == Some(&path),
                    path,
                    folder,
                    focus,
                }
            })
            .collect(),
    );
    order
}

mod view;
