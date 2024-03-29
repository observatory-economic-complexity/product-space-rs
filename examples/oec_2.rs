use csv;
use failure::{Error, format_err};
use nalgebra::DMatrix;
use product_space::{self, ProductSpace, Mcp};
use serde::Deserialize;
use simple_timer::timeit;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::fs::File;
use structopt::StructOpt;

fn main() -> Result<(), Error> {
    let opt = CliOpt::from_args();

    println!("Reading data from: {:?}", opt.filepath);


    let ps = timeit!("ingest-and-init-product-space",
        ps_from_tsv_reader(opt.filepath)?
    );

    println!("");

    timeit!("rca 1yr test",
        {
            let rca = ps.rca(&[2017], None)
                .ok_or_else(|| format_err!("no rca for 2017?"))?;
            println!("RCA test against simoes ps_calcs for 2017");
            println!("nzl::0204, expect 149.962669: {:?}", rca.get("nzl", "0204")?);
        }
    );

    println!("\n## usa::0101 rca\n");

    timeit!("rca 1yr no cutoff 1.0, x3",
        for year in 2015..=2017 {
            let rca = ps.rca(&[year as u32], None)
                .ok_or_else(|| format_err!("no rca for year"))?;
            println!("usa::0101, {}: {}", year, rca.get("usa", "0101")?);
        }
    );

    println!("");

    timeit!("rca 3yr cutoff 1.0",
        {
            let rca = ps.rca(&[2015,2016,2017], Some(1.0))
                .ok_or_else(|| format_err!("no rca for 2015-2017?"))?;
            println!("usa::0101, 2015-2017: {}", rca.get("usa", "0101")?);
        }
    );

    println!("\n## usa::0101 density\n");

    timeit!("density 1yr cutoff 1.0, x3",
        for year in 2015..=2017 {
            let density = ps.density(&[year], Some(1.0))
                .ok_or_else(|| format_err!("no rca for 2017?"))?;
            println!("usa::0101, {}: {:?}", year, density.get("usa", "0101")?);
        }
    );

    println!("");

    timeit!("density 3yr cutoff 1.0",
        {
            let density = ps.density(&[2015,2016,2017], Some(1.0))
                .ok_or_else(|| format_err!("no rca for 2015-2017?"))?;
            println!("usa::0101, 2015-2017: {}", density.get("usa", "0101")?);
        }
    );

    println!("");


    Ok(())
}

/// Constructed
/// - on country exports by product
/// - skipping null exports
///
/// matrix:
/// - row = countries
/// - columns = products
pub fn ps_from_tsv_reader(filepath: PathBuf) -> Result<ProductSpace, Error> {
    // country and product sets are needed while building, because some countries
    // and some products may not exist in each year
    //
    // So this is a preprocessing step before putting everything in the matrix
    let mut country_set = HashSet::new();
    let mut product_set = HashSet::new();
    let mut year_set = HashSet::new();

    // 2 passes are needed, unless files are sorted.
    // But maybe simpler with 2 passes.
    //
    // 1st pass to get all product and countries, to know size of matrices
    // 2nd pass to create matrices.

    // first pass
    let f = File::open(&filepath)?;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(f);

    for result in rdr.deserialize() {
        let record: Record = result?;
        country_set.insert(record.country.to_string());
        product_set.insert(record.product.to_string());
        year_set.insert(record.year);
    }

    // now build all matrics in preparation for mutating
    let mut mcps: HashMap<u32,_> = year_set.into_iter()
        .map(|y| (y, DMatrix::zeros(country_set.len(), product_set.len())))
        .collect();

    let country_idx: HashMap<_,_> = country_set.into_iter()
        .enumerate()
        .map(|(v,k)| (k,v))
        .collect();
    let product_idx: HashMap<_,_> = product_set.into_iter()
        .enumerate()
        .map(|(v,k)| (k,v))
        .collect();

    // now 2nd pass
    let f = File::open(&filepath)?;
    let mut rdr = csv::ReaderBuilder::new()
        .delimiter(b'\t')
        .from_reader(f);

    for result in rdr.deserialize() {
        let record: Record = result?;

        if record.val != "NULL" {
            let export = record.val.parse::<f64>()?;

            let mcp = mcps.get_mut(&record.year)
                .expect("logic error, year must be in");

            let matrix_row_idx = country_idx.get(&record.country)
                .expect("logic error, country must be in");
            let matrix_col_idx = product_idx.get(&record.product)
                .expect("logic error, product must be in");

            let mut matrix_row = mcp.row_mut(*matrix_row_idx);
            // this could be unchecked
            matrix_row[*matrix_col_idx] = export;
        }
    }

    let res = timeit!("init-product-space",
        ProductSpace::new(
            country_idx,
            product_idx,
            mcps,
            Some(1.0),
        )
    );

    Ok(res)
}

#[derive(Debug, Deserialize)]
struct Record {
    #[serde(rename="origin")]
    country: String,
    #[serde(rename="hs92")]
    product: String,
    year: u32,
    #[serde(rename="export_val")]
    val: String, // parse to f64 after, but have to handle NULL
}

#[derive(Debug, StructOpt)]
struct CliOpt {
    #[structopt(parse(from_os_str))]
    filepath: PathBuf,
}

