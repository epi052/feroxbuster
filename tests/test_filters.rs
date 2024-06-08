mod utils;
use assert_cmd::prelude::*;
use httpmock::Method::GET;
use httpmock::MockServer;
use predicates::prelude::*;
use std::process::Command;
use utils::{setup_tmp_directory, teardown_tmp_directory};

#[test]
/// create a FeroxResponse that should elicit a true from
/// StatusCodeFilter::should_filter_response
fn filters_status_code_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(302).body("this is a test");
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/file.js");
        then.status(200).body("this is also a test of some import");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("-vvvv")
        .arg("--filter-status")
        .arg("302")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .not()
            .and(predicate::str::contains("302"))
            .not()
            .and(predicate::str::contains("14c"))
            .not()
            .and(predicate::str::contains("/file.js"))
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("34c")),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// LinesFilter::should_filter_response
fn filters_lines_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(302).body("this is a test");
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/file.js");
        then.status(200)
            .body("this is also a test of some import\nwith 2 lines, no less");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-lines")
        .arg("2")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14"))
            .and(predicate::str::contains("/file.js"))
            .not()
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("2l"))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// WordsFilter::should_filter_response
fn filters_words_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(302).body("this is a test");
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/file.js");
        then.status(200)
            .body("this is also a test of some import\nwith 2 lines, no less");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-words")
        .arg("13")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14"))
            .and(predicate::str::contains("/file.js"))
            .not()
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("13w"))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// SizeFilter::should_filter_response
fn filters_size_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) =
        setup_tmp_directory(&["LICENSE".to_string(), "file.js".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE");
        then.status(302).body("this is a test");
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/file.js");
        then.status(200)
            .body("this is also a test of some import\nwith 2 lines, no less");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-size")
        .arg("56")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14"))
            .and(predicate::str::contains("/file.js"))
            .not()
            .and(predicate::str::contains("200"))
            .not()
            .and(predicate::str::contains("56c"))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// SimilarityFilter::should_filter_response
fn filters_similar_should_filter_response() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(
        &["not-similar".to_string(), "similar".to_string()],
        "wordlist",
    )
    .unwrap();

    // ''.join(random.choices(string.ascii_letters + string.digits + string.whitespace, k=4096))
    let content = "VCiYFr0HKsEIK6r\r1hJLYnOr90Aji\rDWAjQA3LVAzrluN48FuSPrRpm\n \tV\x0cx\nSCc5sX\nTB\x0c6Of7ns\t2HDwQCduKTqG8gG\x0beszazwljW01H60HMOLziOKwQwEYV7CbrLWQiLeCWKVxX\rvag\nAAEOhjER7gURuGXw\nMyY\t8mSw\x0b\x0bK0Z9G0Pt\x0bJZItAIqAq FxeaoOeLqWVFvxtDFfko0YVYt1I\rNmSXZ4lnOoiBCLbu6TLb80lClhY\tPN7Lp36F786I\nglwRK2oD45EtN SWW IF6uqKdf\x0czAcVycf\x0cBzHYnn1HAkU2Jluos0qwMGJ2m74z\nLd3\x0cIUVZmnRmHHWQGd1u2xmsZR\x0bfnml10ur6J\x0ba8xOZatiY 15Aq3KOGWdD3xQwqo\r5SKnnxH5tqU\rO\rZpJ\n7t7UUgfE\niWFgqWDpMeOG 1248M I\ro5B9Yed\r2aq2\tXxLn31s3hCV WEfQd60DKp6eFhUeUSeXDq6qjgTnWigoCZQERf\rXp7s2L37 iOEMl3\r41\nBShOjLfD8Kj0\rbu0ENreRjP\nY77jsrsaYgOsUrEzw\x0bw3OLi\n8fkddcaOvJeutTy B\rsDMkK\x0cnx2S0N\x0cDaY\x0c9iyo6p4IL\tOC1qgNlWP4VLg\tWmPG46ZMCirth5h4FwkS\nD2WsiEA2Z\n0xbLd7Uww hUQC6 3V\r1SsWem4UcQxG\rfuVvWl\nD9\nDpZQFFgiqhQiq1I0LMAR\r\rKBmj4iurrxaoMHTl9oj\x0b0N3AfD17gyqZiJ67bgizvecsRGeB1f\x0c\nYRvieJqIVHDKOOR\ruhqnVZz4BQ5FFBusz\x0cZl5\x0bt\tbdOUhAAAKyA6Jwl 7OjzojiRHGD6dl ncsgndsKURhFv4\tV5d\n73iPzbT\t8v6IrJtnq\nJuFl7A\x0b\rVnnsjTW0Y4QB1BgCy3B\x0cma7\tpPt5jmcJH7v5J\tYKEXh UqRChBFY5nbFbmXjJYxevPYJmSHC\rDQ4j9de\rTMZ\rtWaPAzkJjH\x0c\nyrEuf9WaMM\trFlKo9r9w\r\nQkQqIEu8Gfr\t aRzvN\r2oZhCyB4fa\np37\tXQi4Wa\no7gHUDQLoRvkK1dy2K3ydrI0O6\rFTGS7oHA\x0bajFOd\rcS5W25tFGhocwxM0\nuugNGDLjBQ\tWGdJV0\x0c\r7bNLs\x0cr deAWt35A4co\x0bPCuYmQ ExxtK\rvpckCyJxLrK5xULK\x0cvqtiGyovHQW8aDjV3rhXhR\nmQvmK\x0czLx\x0cECSYSF5jP35zN VkaRzQ lZ4 l06X4HHpsVn 8y8fGbIP\tRWFUAeFI24\rqN\x0cBW7u7WPMv36BmkgzQ\x0c2\x0cyLf\tYo8iRjE7zMsceym4ZnWg7EsOedh2cES\rz2n\x0cJi52uIPfSkAPzW\rEekjgWdb8y 285F4xae8\n8AiIkT4l3AOy\rT4yeXgaRMCI4t3PkHeFZ\rEb6R4FNCE \nbVil\x0c6qxSVPnU\nh\ttFMNE4\x0c\rwF\t\x0cW5vebbRWG\x0biVZLP\x0ct\x0c5gQ4CJ9KJl\x0cwyIfSIYaCvi4m1r\tJbYqmI0NVO36A\t8BSPNlaKbR73l9mxZxoqD4yca\n5h\r7a0z\tVm34aTy\tnLj5nSrh8er5lN0J7hcjmUk2DL\nyWEVNXTF8RWfC\x0bpcgBQXOQzidyYO\x0bh76UyUPAjELmNoECgGq06hiFGDI LiPZcofhcm\r62fEixIoyG\tmI\x0cYLQvBCbCluGgbm\x0c7GI6\n19il8PdPqss2uQqA5KgkHMIb hh211YuqV9kdmVnwyD63pz3p t58q6kHX\r\teYBrg6eDh\x0bx8\x0cI1SOV3Gt5qubmixHR\rApbgkTQJQ\tX0t11IP55hys2d\x0bF dh7j7G0Ac\x0bQMNvkSU9AV\x0b8mcIPHy9d\x0cyINf5qu\x0cdiBFrhiNRmCZ4r\tSx4N5VOm6KCp2T8bOVEjOR6otPAN5e\n\x0csyJ3giBjkgg 9dYQKq5P75AG5\x0bfD6zZO4DxQ44uX7Kz50dv4ncXQA\rqgHT\rLRcsRl\rW\t7We\tpAEJHMChxwVK\x0cprVvINvolf7hj\tUrob\rW3pXlqKIEQT8t7\x0bGODJanb328OiQCxE\rPfW4j\rl3p\x0cRXDB55u0MN7isBL\ty3UvE1 7I\nfuoZVPzk7az1\rMzA2FROXu0k\rFq pby6pHMqfTQT7iTw izlk0CUpyoUaq5w3UPFK7\rMOPw2cZ6FsVITbCoPhT\rIvuImCFGqmYpE hNevWkPCtwwnx2sX\x0c7oKzBExp32ZpdY\tstuDjSzfalsO1M\x0bNMUegnBDr3Liv3Lv\x0b\n37VZT2LEJ9fNYDi9r\t\x0bYC\rHSt0oJbk\x0b\x0bUdS8eB\nMXBPDEppZjHR7vGZYqX7yFm t1i682AXWf VPTzYTvm6mhOre8\x0bk0spJNYuI\tk\tC1B1N0 AYYDWH\t\tX1TjinXdkXcbFTlIiBLzx\rmUoyx9b7paJSVMX\tfLo8hU1Dmuluyk8R8\x0c4\x0cBe\nCrIMlyek4i\x0bFwuE9\nXUqpVxikH0PZspopUwPM9Kcue\rBh2Mf\rme3h4qelC\x0bEH\x0bkkxi6U\x0cE\x0ctqBgN93 V4ovmocLrK6\ngCQlf\x0cshRVvrPq\x0cOjgbjhSEK8PIx8OYqjjDDkJ0AgLhfbdGw2\nLMv2M0E08PGXnqUyVsjN\t C 4\n80 Fia g\x0b5dEFvyl5Y80U6sMAdHgk2nzC5ElDBhgcBprXC\x0bIMKXyt\x0ce5SkYcRartfblLqD1 A5\nre\x0bj67lJYCs\t8b50xA69eMHqGDLLP8sJceN19kkonjLj\t\rS\tk9sMOeewQHbT \x0cp53aMX9\x0bDYCZWAtdA6h\rAFHDEYFBE1MzdOxMO\x0cvDE7QfLb3jq4s\tI3aVTmDDOQAnuvWb2AGUUP\rf2HinUAiF13LKEfpqcD06S8aQC0Kyl729L7a6CbuoB0GRlJx tD yuTVqD62HuXpfKrDsbejEdp3\rxjc\x0bn4lLNaViizec\rWR\x0cTT5aZ\ny9\rO1qB1XGQPnES\nUhJtU Ll7t3Zglj1IAEx 8Rh3V\x0bfmUSC4\x0bVR9l33LS3bPAJpLbH3Q2\nv2fqMeIt3nGR\x0cgCixM4qzVSx7Yb192a1HWx8nnuWQIEK7QHL6p\x0cD3d0Y1FoZqsmY2U\rspvt3gwKOHR6RaZlmhX\n3bmIEF6\x0b\x0bMXJKOnXPgjkdhun4aGDBw\x0cOEW\repDYTcc48oZ4lg7PukNq7TU\tWP0ZJbzVKK\rxAMaZujwTqQXsXODiE2DdwnstAa6CMYfzj7J\x0c2Q\tY2764IYCy 3Fqm0\x0ckbe7VvfqWUh0\tUlubxZ\rX59MfNSfCfcH8GFZIGIRPt\rZVXfra1 H7VI2yJ\x0cspGDCi\rcgHfZa8528CP9tilUx0ifWPGqskLVDPLJP\nciNxodMQSrJXp\ro\r9aBFHCV\x0cR\rrp\x0bmMfxg5rG\tSuWonbJQlmHQ\ri34w8S\x0cN9Ezj2k2OmLH\x0cEcVUDjXNZIFCtlA843I44p GZyhlOctwpd7 OZnUxk4uacN\r8NihNGO\n9eXy5l6gQe5srySxxvuX5jtCzuJ35xvCfEXYa\x0b2lTDBOAaSYpnl v9L\x0cY8RLg2oE7xeCUbD\tSHKZgeXHZIzYAmA7bsmiZUfzmo5ZZUhtBh4F\x0bTx1\x0bz zQov5mYwfpWJTR2Q\x0bLRXMuBzj\x0bZC\x0b pFNPj8ixWJQggQlr9eNW6SHLJk731nc\x0cBn\x0ckQxg2BdRT\x0bp6lf7G\x0bnIMDeY8w6fUf\x0cjGE1Pfsekv7EYEIHsOAsZb3lBfBPO9\tXpHPBMRmRtzMc5WoX6C5cc\x0cBuTPtPOgXnap1Y3xq7pcMcgu55xblsXEAJKsojjR7aDB\tU84kUKRNEj\n8mcqEyOmvq1WA\na6bhzYf9VQv2aj9KLfByVqUKNFVIc4Mkha\x0c0aCPQSKe0GGwPlSfbtNXhdhxAb3RLf1J\x0cshJzjQe4DCmlRmjt\tlB0BwzBpkg2hTYM\r S\x0cux\x0bj6IcEZ\n\ngQ\rKKgg \rrv4sUMy5sfY1aatjK1MmUyXR\rRHk\x0cqq\x0cD1fy4C0\n\x0byd4SFKOyKJqx2mzI74vPxLLo\x0c0OamjXuUu\nWGkiA70nuf0PGRfwLEBPCMeyneJI1HcIXH\nCTFEIMiAq6fT\rmJgC hXEU\rriAhCm3OzgbcDgvQgDSyUw5jl\x0cTaLOPuFseq\x0cj2npTd57itktTdWBY7sqlOGKNSc\x0ctx2mUoHi31EF3l5lvYPDeG6bIPFwIn7\tG6G \x0bgNkSn89flvqcvI73RA";
    let mutated = "VCiYFr0HKsEIK6r\r1hJLYnOr90Aji\rDWAjQA3LVAzrluN484327FuSPrRpm\n \tV\x0cx\nSCc5sX\nTB\x0c6Of7ns\t2HDwQCduKTqG8gG\x0beszazwljW01H60HMOLziOKwQwEYV7CbrLWQiLeCWKVxX\rvag\nAAEOhjER7gURuGXw\nMyY\t8mSw\x0b\x0bK0Z9G0Pt\x0bJZItAIqAq FxeaoOeLqWVFvxtDFfko0YVYt1I\rNmSXZ4lnOoiBCLbu6TLb80lClhY\tPN7Lp36F786I\nglwRK2oD45EtN SWW IF6uqKdf\x0czAcVycf\x0cBzHYnn1HAkU2Jluos0qwMGJ2m74z\nLd3\x0cIUVZmnRmHHWQGd1u2xmsZR\x0bfnml10ur6J\x0ba8xOZatiY 15Aq3KOGWdD3xQwqo\r5SKnnxH5tqU\rO\rZpJ\n7t7UUgfE\niWFgqWDpMeOG 1248M I\ro5B9Yed\r2aq2\tXxLn31s3hCV WEfQd60DKp6eFhUeUSeXDq6qjgTnWigoCZQERf\rXp7s2L37 iOEMl3\r41\nBShOjLfD8Kj0\rbu0ENreRjP\nY77jsrsaYgOsUrEzw\x0bw3OLi\n8fkddcaOvJeutTy B\rsDMkK\x0cnx2S0N\x0cDaY\x0c9iyo6p4IL\tOC1qgNlWP4VLg\tWmPG46ZMCirth5h4FwkS\nD2WsiEA2Z\n0xbLd7Uww hUQC6 3V\r1SsWem4UcQxG\rfuVvWl\nD9\nDpZQFFgiqhQiq1I0LMAR\r\rKBmj4iurrxaoMHTl9oj\x0b0N3AfD17gyqZiJ67bgizvecsRGeB1f\x0c\nYRvieJqIVHDKOOR\ruhqnVZz4BQ5FFBusz\x0cZl5\x0bt\tbdOUhAAAKyA6Jwl 7OjzojiRHGD6dl ncsgndsKURhFv4\tV5d\n73iPzbT\t8v6IrJtnq\nJuFl7A\x0b\rVnnsjTW0Y4QB1BgCy3B\x0cma7\tpPt5jmcJH7v5J\tYKEXh UqRChBFY5nbFbmXjJYxevPYJmSHC\rDQ4j9de\rTMZ\rtWaPAzkJjH\x0c\nyrEuf9WaMM\trFlKo9r9w\r\nQkQqIEu8Gfr\t aRzvN\r2oZhCyB4fa\np37\tXQi4Wa\no7gHUDQLoRvkK1dy2K3ydrI0O6\rFTGS7oHA\x0bajFOd\rcS5W25tFGhocwxM0\nuugNGDLjBQ\tWGdJV0\x0c\r7bNLs\x0cr deAWt35A4co\x0bPCuYmQ ExxtK\rvpckCyJxLrK5xULK\x0cvqtiGyovHQW8aDjV3rhXhR\nmQvmK\x0czLx\x0cECSYSF5jP35zN VkaRzQ lZ4 l06X4HHpsVn 8y8fGbIP\tRWFUAeFI24\rqN\x0cBW7u7WPMv36BmkgzQ\x0c2\x0cyLf\tYo8iRjE7zMsceym4ZnWg7EsOedh2cES\rz2n\x0cJi52uIPfSkAPzW\rEekjgWdb8y 285F4xae8\n8AiIkT4l3AOy\rT4yeXgaRMCI4t3PkHeFZ\rEb6R4FNCE \nbVil\x0c6qxSVPnU\nh\ttFMNE4\x0c\rwF\t\x0cW5vebbRWG\x0biVZLP\x0ct\x0c5gQ4CJ9KJl\x0cwyIfSIYaCvi4m1r\tJbYqmI0NVO36A\t8BSPNlaKbR73l9mxZxoqD4yca\n5h\r7a0z\tVm34aTy\tnLj5nSrh8er5lN0J7hcjmUk2DL\nyWEVNXTF8RWfC\x0bpcgBQXOQzidyYO\x0bh76UyUPAjELmNoECgGq06hiFGDI LiPZcofhcm\r62fEixIoyG\tmI\x0cYLQvBCbCluGgbm\x0c7GI6\n19il8PdPqss2uQqA5KgkHMIb hh211YuqV9kdmVnwyD63pz3p t58q6kHX\r\teYBrg6eDh\x0bx8\x0cI1SOV3Gt5qubmixHR\rApbgkTQJQ\tX0t11IP55hys2d\x0bF dh7j7G0Ac\x0bQMNvkSU9AV\x0b8mcIPHy9d\x0cyINf5qu\x0cdiBFrhiNRmCZ4r\tSx4N5VOm6KCp2T8bOVEjOR6otPAN5e\n\x0csyJ3giBjkgg 9dYQKq5P75AG5\x0bfD6zZO4DxQ44uX7Kz50dv4ncXQA\rqgHT\rLRcsRl\rW\t7We\tpAEJHMChxwVK\x0cprVvINvolf7hj\tUrob\rW3pXlqKIEQT8t7\x0bGODJanb328OiQCxE\rPfW4j\rl3p\x0cRXDB55u0MN7isBL\ty3UvE1 7I\nfuoZVPzk7az1\rMzA2FROXu0k\rFq pby6pHMqfTQT7iTw izlk0CUpyoUaq5w3UPFK7\rMOPw2cZ6FsVITbCoPhT\rIvuImCFGqmYpE hNevWkPCtwwnx2sX\x0c7oKzBExp32ZpdY\tstuDjSzfalsO1M\x0bNMUegnBDr3Liv3Lv\x0b\n37VZT2LEJ9fNYDi9r\t\x0bYC\rHSt0oJbk\x0b\x0bUdS8eB\nMXBPDEppZjHR7vGZYqX7yFm t1i682AXWf VPTzYTvm6mhOre8\x0bk0spJNYuI\tk\tC1B1N0 AYYDWH\t\tX1TjinXdkXcbFTlIiBLzx\rmUoyx9b7paJSVMX\tfLo8hU1Dmuluyk8R8\x0c4\x0cBe\nCrIMlyek4i\x0bFwuE9\nXUqpVxikH0PZspopUwPM9Kcue\rBh2Mf\rme3h4qelC\x0bEH\x0bkkxi6U\x0cE\x0ctqBgN93 V4ovmocLrK6\ngCQlf\x0cshRVvrPq\x0cOjgbjhSEK8PIx8OYqjjDDkJ0AgLhfbdGw2\nLMv2M0E08PGXnqUyVsjN\t C 4\n80 Fia g\x0b5dEFvyl5Y80U6sMAdHgk2nzC5ElDBhgcBprXC\x0bIMKXyt\x0ce5SkYcRartfblLqD1 A5\nre\x0bj67lJYCs\t8b50xA69eMHqGDLLP8sJceN19kkonjLj\t\rS\tk9sMOeewQHbT \x0cp53aMX9\x0bDYCZWAtdA6h\rAFHDEYFBE1MzdOxMO\x0cvDE7QfLb3jq4s\tI3aVTmDDOQAnuvWb2AGUUP\rf2HinUAiF13LKEfpqcD06S8aQC0Kyl729L7a6CbuoB0GRlJx tD yuTVqD62HuXpfKrDsbejEdp3\rxjc\x0bn4lLNaViizec\rWR\x0cTT5aZ\ny9\rO1qB1XGQPnES\nUhJtU Ll7t3Zglj1IAEx 8Rh3V\x0bfmUSC4\x0bVR9l33LS3bPAJpLbH3Q2\nv2fqMeIt3nGR\x0cgCixM4qzVSx7Yb192a1HWx8nnuWQIEK7QHL6p\x0cD3d0Y1FoZqsmY2U\rspvt3gwKOHR6RaZlmhX\n3bmIEF6\x0b\x0bMXJKOnXPgjkdhun4aGDBw\x0cOEW\repDYTcc48oZ4lg7PukNq7TU\tWP0ZJbzVKK\rxAMaZujwTqQXsXODiE2DdwnstAa6CMYfzj7J\x0c2Q\tY2764IYCy 3Fqm0\x0ckbe7VvfqWUh0\tUlubxZ\rX59MfNSfCfcH8GFZIGIRPt\rZVXfra1 H7VI2yJ\x0cspGDCi\rcgHfZa8528CP9tilUx0ifWPGqskLVDPLJP\nciNxodMQSrJXp\ro\r9aBFHCV\x0cR\rrp\x0bmMfxg5rG\tSuWonbJQlmHQ\ri34w8S\x0cN9Ezj2k2OmLH\x0cEcVUDjXNZIFCtlA843I44p GZyhlOctwpd7 OZnUxk4uacN\r8NihNGO\n9eXy5l6gQe5srySxxvuX5jtCzuJ35xvCfEXYa\x0b2lTDBOAaSYpnl v9L\x0cY8RLg2oE7xeCUbD\tSHKZgeXHZIzYAmA7bsmiZUfzmo5ZZUhtBh4F\x0bTx1\x0bz zQov5mYwfpWJTR2Q\x0bLRXMuBzj\x0bZC\x0b pFNPj8ixWJQggQlr9eNW6SHLJk731nc\x0cBn\x0ckQxg2BdRT\x0bp6lf7G\x0bnIMDeY8w6fUf\x0cjGE1Pfsekv7EYEIHsOAsZb3lBfBPO9\tXpHPBMRmRtzMc5WoX6C5cc\x0cBuTPtPOgXnap1Y3xq7pcMcgu55xblsXEAJKsojjR7aDB\tU84kUKRNEj\n8mcqEyOmvq1WA\na6bhzYf9VQv2aj9KLfByVqUKNFVIc4Mkha\x0c0aCPQSKe0GGwPlSfbtNXhdhxAb3RLf1J\x0cshJzjQe4DCmlRmjt\tlB0BwzBpkg2hTYM\r S\x0cux\x0bj6IcEZ\n\ngQ\rKKgg \rrv4sUMy5sfY1aatjK1MmUyXR\rRHk\x0cqq\x0cD1fy4C0\n\x0byd4SFKOyKJqx2mzI74vPxLLo\x0c0OamjXuUu\nWGkiA70nuf0PGRfwLEBPCMeyneJI1HcIXH\nCTFEIMiAq6fT\rmJgC hXEU\rriAhCm3OzgbcDgvQgDSyUw5jl\x0cTaimauFseq\x0cj2npTd57itktTdWBY7sqlOGKNSc\x0ctx2mUoHi31EF3l5lvYPDeG6bIPFwIn7\tG6G \x0bgNkSn89flvqcvI73RA";

    let canary = srv.mock(|when, then| {
        when.method(GET).path("/canary");
        then.status(200).body(content);
    });

    // not similar, should see results in output
    let not_similar = srv.mock(|when, then| {
        when.method(GET).path("/not-similar");
        then.status(302).body("this is a test");
    });

    // similar, should not see results
    let similar = srv.mock(|when, then| {
        when.method(GET).path("/similar");
        then.status(200).body(mutated);
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-similar-to")
        .arg(srv.url("/canary"))
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICfdafdsafdsafadsENSE")
            .and(predicate::str::contains("302"))
            .and(predicate::str::contains("14c"))
            .and(predicate::str::contains("/similar"))
            .not()
            .and(predicate::str::contains("4100c"))
            .not(),
    );

    assert_eq!(canary.hits(), 1);
    assert_eq!(similar.hits(), 1);
    assert_eq!(not_similar.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// when using --collect-backups, should only see results in output
/// when the response shouldn't be otherwise filtered
fn collect_backups_should_be_filtered() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(&["LICENSE".to_string()], "wordlist").unwrap();

    let mock = srv.mock(|when: httpmock::When, then| {
        when.method(GET).path("/LICENSE");
        then.status(200).body("this is a test");
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/LICENSE.bak");
        then.status(201)
            .body("im a backup file, but filtered out because im not 200");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--status-codes")
        .arg("200")
        .arg("--collect-backups")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/LICENSE")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("/LICENSE.bak"))
            .not()
            .and(predicate::str::contains("201"))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}

#[test]
/// create a FeroxResponse that should elicit a true from
/// RegexFilter::should_filter_response
fn filters_regex_should_filter_response_based_on_headers() {
    let srv = MockServer::start();
    let (tmp_dir, file) = setup_tmp_directory(
        &["not-matching".to_string(), "matching".to_string()],
        "wordlist",
    )
    .unwrap();

    let mock = srv.mock(|when, then| {
        when.method(GET).path("/not-matching");
        then.status(200)
            .header("content-type", "text/html")
            .body("this is a test");
    });

    let mock_two = srv.mock(|when, then| {
        when.method(GET).path("/matching");
        then.status(200)
            .header("content-type", "application/json")
            .body("this is also a test");
    });

    let cmd = Command::cargo_bin("feroxbuster")
        .unwrap()
        .arg("--url")
        .arg(srv.url("/"))
        .arg("--wordlist")
        .arg(file.as_os_str())
        .arg("--filter-regex")
        .arg("content-type:application/json")
        .unwrap();

    cmd.assert().success().stdout(
        predicate::str::contains("/not-matching")
            .and(predicate::str::contains("200"))
            .and(predicate::str::contains("/matching"))
            .not()
            .and(predicate::str::contains("200"))
            .not(),
    );

    assert_eq!(mock.hits(), 1);
    assert_eq!(mock_two.hits(), 1);
    teardown_tmp_directory(tmp_dir);
}
