// use cosmwasm_std::Api;
// use cw_asset::AssetInfo;
// use euclid::error::ContractError;

// /// Helper function to check if the given asset infos are valid.
// pub(crate) fn check_asset_infos(
//     api: &dyn Api,
//     asset_infos: &[AssetInfo],
// ) -> Result<(), ContractError> {
//     if !asset_infos.iter().all_unique() {
//         return Err(ContractError::InvalidAsset {
//             asset: "".to_string(),
//         });
//     }

//     asset_infos
//         .iter()
//         .try_for_each(|asset_info| asset_info.check(api))
//         .map_err(Into::into)
// }
