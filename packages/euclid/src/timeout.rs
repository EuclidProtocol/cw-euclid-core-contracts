use cosmwasm_std::ensure;

use crate::error::ContractError;

/// Ensures that the timeout is between 30 and 240 seconds. It defaults to 60 seconds if the timeout isn't provided.
pub fn get_timeout(timeout: Option<u64>) -> Result<u64, ContractError> {
    if let Some(timeout) = timeout {
        // Validate that the timeout is between 30 and 240 seconds inclusive
        ensure!(
            timeout.ge(&30) && timeout.le(&240),
            ContractError::InvalidTimeout {}
        );
        Ok(timeout)
    } else {
        // Default timeout to 60 seconds if not provided
        Ok(60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestGetTimeout {
        name: &'static str,
        timeout: Option<u64>,
        expected_error: Option<ContractError>,
        expected_result: Option<u64>,
    }

    #[test]
    fn test_get_timeout() {
        let test_cases = vec![
            TestGetTimeout {
                name: "Empty timeout",
                timeout: None,
                expected_error: None,
                expected_result: Some(60),
            },
            TestGetTimeout {
                name: "Timeout below 30",
                timeout: Some(29),
                expected_error: Some(ContractError::InvalidTimeout {}),
                expected_result: None,
            },
            TestGetTimeout {
                name: "Timeout above 240",
                timeout: Some(241),
                expected_error: Some(ContractError::InvalidTimeout {}),
                expected_result: None,
            },
            TestGetTimeout {
                name: "Timeout at 240",
                timeout: Some(240),
                expected_error: None,
                expected_result: Some(240),
            },
            TestGetTimeout {
                name: "Timeout at 30",
                timeout: Some(30),
                expected_error: None,
                expected_result: Some(30),
            },
            TestGetTimeout {
                name: "Timeout between 30 and 240",
                timeout: Some(80),
                expected_error: None,
                expected_result: Some(80),
            },
        ];

        for test in test_cases {
            let res = get_timeout(test.timeout);

            if let Some(err) = test.expected_error {
                assert_eq!(res.unwrap_err(), err, "{}", test.name);
                continue;
            } else {
                assert_eq!(res.unwrap(), test.expected_result.unwrap())
            }
        }
    }
}
