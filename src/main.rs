use hashbrown::HashSet;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, Hash};
use twox_hash::XxHash64;

use std::f64::consts::TAU;

use image::{ImageBuffer, RgbImage};
use rand::prelude::*;

type Color = [u8; 3];
type ColorBase = [u8; 3];

fn color_base_to_color(cb: ColorBase, color_size: u64) -> Color {
    cb.map(|cbc| (cbc as u64 * 255 / (color_size - 1)) as u8)
}
type ColorOffset = [i16; 3];
type Location = [usize; 2];

fn make_bases_offsets<R: Rng>(scale: u64, rng: &mut R) -> (Vec<ColorBase>, Vec<ColorOffset>) {
    let color_size = scale.pow(2);
    let mut color_bases: Vec<ColorBase> = (0..scale.pow(6))
        .map(|n| {
            let r_base = n % color_size;
            let g_base = (n / color_size) % color_size;
            let b_base = n / color_size.pow(2);
            [r_base as u8, g_base as u8, b_base as u8]
        })
        .collect();
    let mut color_offsets: Vec<ColorOffset> = color_bases
        .iter()
        .map(|color| color.map(|c| c as i16))
        .flat_map(|color| {
            vec![
                [color[0], color[1], color[2]],
                [color[0], color[1], -color[2]],
                [color[0], -color[1], color[2]],
                [color[0], -color[1], -color[2]],
                [-color[0], color[1], color[2]],
                [-color[0], color[1], -color[2]],
                [-color[0], -color[1], color[2]],
                [-color[0], -color[1], -color[2]],
            ]
            .into_iter()
        })
        .collect();
    color_bases.shuffle(rng);
    color_offsets
        .sort_by_key(|color_offset| color_offset.map(|c| (c as i64).pow(2)).iter().sum::<i64>());
    (color_bases, color_offsets)
}

fn remove_random<T, H, R>(set: &mut HashSet<T, H>, rng: &mut R) -> Option<T>
where
    R: Rng,
    T: Eq + PartialEq + Hash,
    H: BuildHasher,
{
    if set.is_empty() {
        return None;
    }
    if set.capacity() >= 8 && set.len() < set.capacity() / 4 {
        set.shrink_to_fit();
    }
    let raw_table = set.raw_table_mut();
    let num_buckets = raw_table.buckets();
    loop {
        let bucket_index = rng.gen_range(0..num_buckets);
        // Safety: bucket_index is less than the number of buckets.
        // Note that we return the first time we modify the table,
        // so raw_table.buckets() never changes.
        // Also, the table has been allocated, because set is a HashSet.
        unsafe {
            if raw_table.is_bucket_full(bucket_index) {
                let bucket = raw_table.bucket(bucket_index);
                let ((element, ()), _insert_slot) = raw_table.remove(bucket);
                return Some(element);
            }
        }
    }
}

fn array_zip<T, U, const V: usize>(a: [T; V], b: [U; V]) -> [(T, U); V]
where
    T: Copy,
    U: Copy,
{
    std::array::from_fn::<(T, U), V, _>(|i| (a[i], b[i]))
}

fn make_image(
    scale: u64,
    num_seeds: usize,
    initial_turn_rate: f64,
    alpha: f64,
    cycle_cap: usize,
    seed: u64,
) -> RgbImage {
    let mut rng = StdRng::seed_from_u64(seed);
    let size = scale.pow(3) as usize;
    let color_size = scale.pow(2);
    let (color_bases, color_offsets) = make_bases_offsets(scale, &mut rng);
    let mut grid: Vec<Vec<Option<ColorBase>>> = vec![vec![None; size]; size];
    let mut initial_dirs: Vec<Vec<f64>> = vec![vec![0.0; size]; size];
    let mut color_base_to_location: HashMap<ColorBase, Location> = HashMap::new();
    // Fixed hasher because we use the iteration order later
    let mut open_locs: HashSet<Location, BuildHasherDefault<XxHash64>> = (0..size)
        .flat_map(|i| (0..size).map(move |j| [i, j]))
        .collect();
    'main: for (i, color_base) in color_bases.into_iter().enumerate() {
        //let pixel = color_base_to_color(color_base, color_size);
        if i < num_seeds {
            let loc = remove_random(&mut open_locs, &mut rng).expect("Don't over draw");
            grid[loc[0]][loc[1]] = Some(color_base);
            initial_dirs[loc[0]][loc[1]] = rng.gen_range(0.0..TAU);
            color_base_to_location.insert(color_base, loc);
            continue;
        }
        let most_similar_location: Location = color_offsets
            .iter()
            .filter_map(|color_offset| {
                let prov_new_color_base =
                    array_zip(color_base, *color_offset).map(|(c, co)| c as i16 + co);
                if prov_new_color_base.iter().any(|&c| c < 0 || c > 255) {
                    None
                } else {
                    let new_color_base = prov_new_color_base.map(|c| c as u8);
                    color_base_to_location.get(&new_color_base).copied()
                }
            })
            .next()
            .expect("Seeded");
        let mut dir = initial_dirs[most_similar_location[0]][most_similar_location[1]];
        let mut loc = most_similar_location.map(|i| i as f64);
        for step in 1..cycle_cap*size {
            // Update loc
            loc[0] += dir.sin();
            loc[1] += dir.cos();
            // Update dir
            dir += initial_turn_rate / (step as f64).powf(alpha);
            // Round loc, check if open
            let pos = loc.map(|f| (f - (f / size as f64).floor() * size as f64) as usize);
            assert!(pos[0] < size);
            assert!(pos[1] < size);
            if grid[pos[0]][pos[1]].is_none() {
                grid[pos[0]][pos[1]] = Some(color_base);
                initial_dirs[pos[0]][pos[1]] = dir;
                color_base_to_location.insert(color_base, pos);
                let was_present = open_locs.remove(&pos);
                assert!(was_present);
                continue 'main;
            }
        }
        let loc = remove_random(&mut open_locs, &mut rng).expect("Don't over draw later");
        grid[loc[0]][loc[1]] = Some(color_base);
        initial_dirs[loc[0]][loc[1]] = rng.gen_range(0.0..TAU);
        color_base_to_location.insert(color_base, loc);
    }
    let mut img: RgbImage = ImageBuffer::new(size as u32, size as u32);
    for (i, row) in grid.into_iter().enumerate() {
        for (j, color_base) in row.into_iter().enumerate() {
            if let Some(color_base) = color_base {
                img.put_pixel(
                    i as u32,
                    j as u32,
                    image::Rgb(color_base_to_color(color_base, color_size)),
                );
            }
        }
    }
    img
}

fn main() {
    let scale = 12;
    let num_seeds = 35;
    let initial_turn_rate = 0.01;
    let alpha = 0.2;
    let cycle_cap = 10;
    let seed = 0;
    let filename = format!(
        "img-{scale}-{num_seeds}-{initial_turn_rate}-{alpha}-{cycle_cap}-{seed}.png"
        );
    println!("Start {filename}");
    let img = make_image(scale, num_seeds, initial_turn_rate, alpha, cycle_cap, seed);
    img.save(&filename).unwrap();
}
