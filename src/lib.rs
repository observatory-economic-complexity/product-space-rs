use nalgebra::DMatrix;
use std::collections::HashMap;

mod mcp;
pub use mcp::Mcp;

mod rca;
pub use rca::{
    apply_fair_share,
    apply_fair_share_into,
    apply_rca,
    fair_share,
    rca,
};

mod proximity;
pub use proximity::proximity;

mod density;
pub use density::density;

mod distance;
pub use distance::distance;

mod complexity;
pub use complexity::complexity;

mod error;
pub use error::Error;

mod smooth;

// Currently just country and product.
// May make this more general in the future
//
// each filtered country list (e.g. by population) would be another product space?
// instead of trying to do that filtering dynamically.
//
// TODO This lets us cache rca and proximity by year, and only have to calculate density on the fly depending on how many years are aggregated for smoothing
//
// TODO tests for density and proximity
//
// TODO separate module for smoothing fns, and then let user choose which
// smoothing so use (rca binary, averaging, etc) over an already-calculated
// set of matrixies.
pub struct ProductSpace {
    country_idx: HashMap<String, usize>,
    product_idx: HashMap<String, usize>,

    #[allow(dead_code)]
    mcps:                HashMap<u32, DMatrix<f64>>,
    rcas_by_year:        HashMap<u32, DMatrix<f64>>,
    rcas_cutoff_by_year: HashMap<u32, DMatrix<f64>>,
    proximities_by_year: HashMap<u32, DMatrix<f64>>,
}

impl ProductSpace {
    /// if years not found, either returns None or silently skips
    /// for aggregating, will either
    /// for cutoff, rca(t) = 1 if rca(t-1) > cutoff and rca(t-2) > cutoff...
    /// - otherwise just average
    pub fn rca(
        &self,
        years: &[u32],
        cutoff: Option<f64>,
        ) -> Option<Rca>
    {
        self.rca_matrix(years, cutoff)
            .map(|m| {
                Rca {
                    country_idx: self.country_idx.clone(),
                    product_idx: self.product_idx.clone(),
                    m,
                }
            })
    }

    fn rca_matrix(
        &self,
        years: &[u32],
        cutoff: Option<f64>,
        ) -> Option<DMatrix<f64>>
    {
        if years.len() > 1 {
            let init_matrix = DMatrix::from_element(
                self.country_idx.len(),
                self.product_idx.len(),
                1.0,
            );

            // for cutoff, rca(t) = 1 if rca(t-1) > cutoff and rca(t-2) > cutoff...
            //
            // else just avg the rca
            let mut res = years.iter()
                // silently removes missing years
                .filter_map(|y| self.rcas_by_year.get(y))
                .fold(init_matrix, |mut z, rca| {
                    let mut rca_matrix = rca.clone();
                    if cutoff.is_some() {
                        apply_fair_share_into(&mut rca_matrix, &mut z, cutoff);
                    } else {
                        // just average as default?
                        // do the sum part here, divide at end
                        z += rca_matrix;
                    }
                    z
                });

            // avg if no cutoff
            if cutoff.is_none() {
                res.apply(|x| x / years.len() as f64)
            }

            Some(res)
        } else if years.len() == 1 {
            // no extra allocation for mcp
            years.get(0)
                .and_then(|y| self.rcas_by_year.get(y))
                .map(|rca| {
                    let mut rca_matrix = rca.clone();
                    if cutoff.is_some() {
                        apply_fair_share(&mut rca_matrix, cutoff);
                    }
                    rca_matrix
                })
        } else {
            None
        }
    }

    /// for working with cutoff-on-init rca only
    pub fn rca_cutoff(
        &self,
        years: &[u32],
        ) -> Option<Rca>
    {
        self.rca_cutoff_matrix(years)
            .map(|m| {
                Rca {
                    country_idx: self.country_idx.clone(),
                    product_idx: self.product_idx.clone(),
                    m,
                }
            })
    }

    fn rca_cutoff_matrix(
        &self,
        years: &[u32],
        ) -> Option<DMatrix<f64>>
    {
        if years.len() > 1 {
            let init_matrix = DMatrix::from_element(
                self.country_idx.len(),
                self.product_idx.len(),
                1.0,
            );

            // for cutoff, rca(t) = 1 if rca(t-1) > cutoff and rca(t-2) > cutoff...
            //
            // else just avg the rca
            let res = years.iter()
                // silently removes missing years
                .filter_map(|y| self.rcas_by_year.get(y))
                .fold(init_matrix, |mut z, rca| {
                    z = z.component_mul(&rca);
                    z
                });

            Some(res)
        } else if years.len() == 1 {
            // no extra allocation for mcp
            years.get(0)
                .and_then(|y| self.rcas_cutoff_by_year.get(y))
                .cloned()
        } else {
            None
        }
    }

    /// if years not found, either returns None or silently skips
    /// for aggregating, will either
    /// for cutoff, rca(t) = 1 if rca(t-1) > cutoff and rca(t-2) > cutoff...
    /// - otherwise just average
    pub fn proximity(
        &self,
        years: &[u32],
        ) -> Option<Proximity>
    {
        self.proximity_matrix(years)
            .map(|m| {
                Proximity {
                    product_idx: self.product_idx.clone(),
                    m,
                }
            })
    }

    fn proximity_matrix(
        &self,
        years: &[u32],
        ) -> Option<DMatrix<f64>>
    {
        if years.len() > 1 {
            let proximities = years.iter()
                // silently removes missing years
                // TODO what happens when no years?
                .filter_map(|y| self.proximities_by_year.get(y));

            let res = smooth::avg(proximities, self.product_idx.len(), years.len());

            Some(res)
        } else if years.len() == 1 {
            // no extra allocation for mcp
            years.get(0)
                .and_then(|y| self.proximities_by_year.get(y))
                .cloned()
        } else {
            None
        }
    }

    pub fn density(
        &self,
        years: &[u32],
        rca_cutoff: Option<f64>,
        ) -> Option<Density>
    {
        self.density_matrix(years, rca_cutoff)
            .map(|m| {
                Density {
                    country_idx: self.country_idx.clone(),
                    product_idx: self.product_idx.clone(),
                    m,
                }
            })
    }

    pub fn density_matrix(
        &self,
        years: &[u32],
        rca_cutoff: Option<f64>,
        ) -> Option<DMatrix<f64>>
    {
        let rca = self.rca_matrix(years, rca_cutoff);
        let proximity = self.proximity_matrix(years);

        if rca.is_some() && proximity.is_some() {
            let rca = rca.unwrap();
            let proximity = proximity.unwrap();

            Some(density(&rca, &proximity))
        } else {
            None
        }
    }
}

impl ProductSpace {
    pub fn new(
        country_idx: HashMap<String, usize>,
        product_idx: HashMap<String, usize>,
        mcps: HashMap<u32, DMatrix<f64>>,
        rca_cutoff: Option<f64>,
        ) -> Self
    {
        let rcas_by_year: HashMap<_,_> = mcps.iter()
            .map(|(year, mcp)| {
                let rca_matrix = rca(&mcp);
                (*year, rca_matrix)
            })
            .collect();

        let rcas_cutoff_by_year: HashMap<_,_> = mcps.iter()
            .map(|(year, mcp)| {
                let mut rca_matrix = rca(&mcp);
                apply_fair_share(&mut rca_matrix, rca_cutoff);

                (*year, rca_matrix)
            })
            .collect();

        let proximities_by_year: HashMap<_,_> = rcas_cutoff_by_year.iter()
            .map(|(year, rca)| {
                let mut prox = proximity(&rca);
                // TODO check if this zeroing is ok
                // This fixed the "everything is Nan issue
                prox.apply(|x| if x.is_nan() { 0.0 } else { x });
                (*year, prox)
            })
            .collect();

        Self {
            country_idx,
            product_idx,
            mcps,
            rcas_by_year,
            rcas_cutoff_by_year,
            proximities_by_year,
        }
    }
}

// TODO put indexes in Arc to avoid copying?
pub struct Rca {
    country_idx: HashMap<String, usize>,
    product_idx: HashMap<String, usize>,
    m: DMatrix<f64>,
}

impl Mcp for Rca {
    fn matrix(&self) -> &DMatrix<f64> {
        &self.m
    }
    fn country_index(&self) -> &HashMap<String, usize> {
        &self.country_idx
    }
    fn product_index(&self) -> &HashMap<String, usize> {
        &self.product_idx
    }
}

// TODO put indexes in Arc to avoid copying?
// TODO figure out how this calc shown publicly.
#[allow(dead_code)]
pub struct Proximity {
    product_idx: HashMap<String, usize>,
    m: DMatrix<f64>,
}

// TODO put indexes in Arc to avoid copying?
pub struct Density {
    country_idx: HashMap<String, usize>,
    product_idx: HashMap<String, usize>,
    m: DMatrix<f64>,
}

impl Mcp for Density {
    fn matrix(&self) -> &DMatrix<f64> {
        &self.m
    }
    fn country_index(&self) -> &HashMap<String, usize> {
        &self.country_idx
    }
    fn product_index(&self) -> &HashMap<String, usize> {
        &self.product_idx
    }
}


#[cfg(test)]
mod test {
    use std::collections::HashMap;
    use super::*;

    #[test]
    fn test_ps_interface() {
        let vals = DMatrix::from_vec(2,3,vec![1.0,2.0,3.0,4.0,5.0,6.0]);
        let mut mcps = HashMap::new();
        mcps.insert(2017, vals);

        let ps = ProductSpace::new(
            [("a".to_string(),0usize), ("b".to_string(),1)].iter().cloned().collect(),
            [("01".to_string(),0usize), ("02".to_string(),1), ("03".to_string(),2)].iter().cloned().collect(),
            mcps,
            Some(0.0),
        );

        let rca = ps.rca(&[2017], None).unwrap();

        let expected = DMatrix::from_vec(2,3,vec![0.7777777777777778,1.1666666666666667,1.0,1.0,1.0606060606060606,0.9545454545454545]);

        assert_eq!(rca.m, expected);

        let val = rca.get("a", "01").unwrap();
        assert_eq!(val, 0.7777777777777778);

        let vals = rca.get_country("b").unwrap();
        assert_eq!(vals, vec![1.1666666666666667, 1.0, 0.9545454545454545]);
    }
}
