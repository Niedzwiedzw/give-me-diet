use crate::{models::GMDLog, parser::FromGMD};
use eyre::Result;

#[test]
#[ignore]
fn test_parses_example_1() -> Result<()> {
    const EXAMPLE: &str = r#"
            product Pasibus Avocadus:
             - protein 10g
         
            2024-01-20
            eat Pasibus Avocadus
            eat 50g of Jogurt grecki lidl
        "#;

    GMDLog::from_gmd(EXAMPLE)?;

    Ok(())
}
