//! The empty window's map of recent folders: a line for each folder they sit in, the folders as
//! its stations, and a folder that holds another line's folders as the interchange that line
//! leaves from, going down and then turning right as the tree's route does.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use crate::{index::compare_paths, paths::display_name};

/// Map units, pixels at 100%: from a line's start to its first station, between stations, and
/// from its last station to its end.
pub const LEAD: f32 = 90.;
pub const STEP: f32 = 140.;
pub const TAIL: f32 = 70.;
/// A line leaving an interchange turns right this far above its own row, at 45 degrees.
pub const BEND: f32 = 30.;

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    /// The folder its stations sit in.
    pub parent: PathBuf,
    /// The interchange it leaves from, when that folder is itself a station.
    pub from: Option<usize>,
    /// Where its run along its row starts and ends; a line from an interchange starts where its
    /// bend ends.
    pub start: f32,
    pub end: f32,
    /// Its stations, left to right.
    pub stops: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Station {
    /// The line it is on, which is also its row.
    pub line: usize,
    pub x: f32,
    /// Another line leaves from it.
    pub interchange: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Network {
    /// Top to bottom, a row each.
    pub lines: Vec<Line>,
    /// One for each folder given, in their order.
    pub stations: Vec<Station>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    Left,
    Right,
    Up,
    Down,
}

/// The folder a folder's station sits in; `/` sits in itself.
fn group_of(folder: &Path) -> &Path {
    folder.parent().unwrap_or(folder)
}

impl Network {
    /// The map of `folders`. Lines that leave no interchange come in the order of their folders'
    /// paths, stations in name order. A line from an interchange comes right below the line it
    /// leaves, the rightmost interchange's first, so no line crosses another.
    pub fn new(folders: &[PathBuf]) -> Self {
        let mut groups: BTreeMap<&Path, Vec<usize>> = BTreeMap::new();
        for (i, folder) in folders.iter().enumerate() {
            groups.entry(group_of(folder)).or_default().push(i);
        }
        let name = |i: usize| display_name(folders[i].file_name().unwrap_or_default());
        for stops in groups.values_mut() {
            stops.sort_by(|&a, &b| compare_paths(&name(a), &name(b)));
        }
        let station = Station {
            line: 0,
            x: 0.,
            interchange: false,
        };
        let mut network = Network {
            lines: Vec::new(),
            stations: vec![station; folders.len()],
        };
        for &parent in groups.keys() {
            let leaves_a_station = folders.iter().any(|f| f == parent && group_of(f) != parent);
            if !leaves_a_station {
                network.lay(parent, None, 0., &groups, folders);
            }
        }
        network
    }

    fn lay(
        &mut self,
        parent: &Path,
        from: Option<usize>,
        start: f32,
        groups: &BTreeMap<&Path, Vec<usize>>,
        folders: &[PathBuf],
    ) {
        let line = self.lines.len();
        let stops = groups[parent].clone();
        for (k, &s) in stops.iter().enumerate() {
            self.stations[s] = Station {
                line,
                x: start + LEAD + STEP * k as f32,
                interchange: false,
            };
        }
        let end = start + LEAD + STEP * stops.len().saturating_sub(1) as f32 + TAIL;
        self.lines.push(Line {
            parent: parent.to_path_buf(),
            from,
            start,
            end,
            stops: stops.clone(),
        });
        for &s in stops.iter().rev() {
            let folder = folders[s].as_path();
            if groups.contains_key(folder) && group_of(folder) != folder {
                self.stations[s].interchange = true;
                let x = self.stations[s].x;
                self.lay(folder, Some(s), x + BEND, groups, folders);
            }
        }
    }

    /// Where the longest line ends.
    pub fn width(&self) -> f32 {
        self.lines.iter().map(|line| line.end).fold(0., f32::max)
    }

    /// The station a key moves to from `from`: Left and Right along its line, stopping at the
    /// ends; Up and Down to the line above or below, onto the station nearest in x (the left one
    /// of two as near).
    pub fn step(&self, from: usize, way: Way) -> usize {
        let Some(station) = self.stations.get(from) else {
            return from;
        };
        let line = &self.lines[station.line];
        let at = line.stops.iter().position(|&s| s == from).unwrap_or(0);
        let row = match way {
            Way::Left => return line.stops[at.saturating_sub(1)],
            Way::Right => return line.stops[(at + 1).min(line.stops.len() - 1)],
            Way::Up => station.line.checked_sub(1),
            Way::Down => Some(station.line + 1),
        };
        let Some(other) = row.and_then(|row| self.lines.get(row)) else {
            return from;
        };
        let off = |s: usize| (self.stations[s].x - station.x).abs();
        other
            .stops
            .iter()
            .copied()
            .min_by(|&a, &b| off(a).total_cmp(&off(b)))
            .unwrap_or(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folders(paths: &[&str]) -> Vec<PathBuf> {
        paths.iter().map(PathBuf::from).collect()
    }

    /// The folders of the design mockup, newest first.
    fn mockup() -> Vec<PathBuf> {
        folders(&[
            "/u/Desktop/tripstore-web/leaf",
            "/u/Desktop/memo",
            "/u/Documents/blog",
            "/u/Desktop/tripstore-web",
            "/u/Desktop/tripstore-web/resin",
            "/u/Documents/papers",
            "/u/Desktop/screenshots",
            "/u/Desktop/tripstore-web/amber",
        ])
    }

    fn names(network: &Network, folders: &[PathBuf], line: usize) -> Vec<String> {
        network.lines[line]
            .stops
            .iter()
            .map(|&s| {
                folders[s]
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn a_line_for_each_parent_with_its_folders_in_name_order() {
        let folders = mockup();
        let network = Network::new(&folders);
        assert_eq!(network.lines.len(), 3);
        assert_eq!(network.lines[0].parent, Path::new("/u/Desktop"));
        assert_eq!(
            names(&network, &folders, 0),
            ["memo", "screenshots", "tripstore-web"]
        );
        assert_eq!(network.lines[2].parent, Path::new("/u/Documents"));
        assert_eq!(names(&network, &folders, 2), ["blog", "papers"]);
        assert_eq!(network.stations[1].x, LEAD);
        assert_eq!(network.stations[6].x, LEAD + STEP);
        assert_eq!(network.lines[0].end, LEAD + 2. * STEP + TAIL);
    }

    #[test]
    fn a_folder_holding_recent_folders_is_their_interchange() {
        let folders = mockup();
        let network = Network::new(&folders);
        let line = &network.lines[1];
        assert_eq!(line.parent, Path::new("/u/Desktop/tripstore-web"));
        assert_eq!(line.from, Some(3));
        assert!(network.stations[3].interchange);
        assert!(!network.stations[1].interchange);
        assert_eq!(names(&network, &folders, 1), ["amber", "leaf", "resin"]);
        // The line leaves tripstore-web, the third station, and runs on after its bend.
        let at = LEAD + 2. * STEP;
        assert_eq!(line.start, at + BEND);
        assert_eq!(network.stations[7].x, at + BEND + LEAD);
        assert_eq!(network.width(), at + BEND + LEAD + 2. * STEP + TAIL);
    }

    #[test]
    fn the_rightmost_interchange_leads_the_lines_below() {
        let folders = folders(&["/u/a", "/u/b", "/u/a/x", "/u/b/y"]);
        let network = Network::new(&folders);
        let parents: Vec<&Path> = network.lines.iter().map(|l| l.parent.as_path()).collect();
        assert_eq!(
            parents,
            [Path::new("/u"), Path::new("/u/b"), Path::new("/u/a")]
        );
    }

    #[test]
    fn arrows_move_along_and_across_the_lines() {
        let folders = mockup();
        let network = Network::new(&folders);
        let (memo, screenshots, tripstore) = (1, 6, 3);
        let (amber, blog, papers) = (7, 2, 5);
        assert_eq!(network.step(memo, Way::Right), screenshots);
        assert_eq!(network.step(tripstore, Way::Right), tripstore);
        assert_eq!(network.step(memo, Way::Left), memo);
        assert_eq!(network.step(memo, Way::Up), memo);
        // Down from tripstore-web: amber, the nearest station of its own line.
        assert_eq!(network.step(tripstore, Way::Down), amber);
        assert_eq!(network.step(amber, Way::Down), papers);
        assert_eq!(network.step(blog, Way::Up), amber);
        assert_eq!(network.step(blog, Way::Down), blog);
    }

    #[test]
    fn the_disk_root_is_a_station_of_its_own() {
        let folders = folders(&["/", "/u"]);
        let network = Network::new(&folders);
        assert_eq!(network.lines.len(), 1);
        assert_eq!(network.lines[0].parent, Path::new("/"));
        assert_eq!(network.lines[0].stops, [0, 1]);
        assert!(Network::new(&[]).lines.is_empty());
    }
}
