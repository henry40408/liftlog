package views

// goldenDigests holds the sha256 of every page the Rust server rendered, one
// per former testdata/<name>.golden.html, over the file bytes exactly as
// captured. assertPageGolden hashes the Go output after mapping the current
// build version back to capturedVersion, so the digests do not move with it.
//
// The pages were captured from the Rust debug build of commit 19c79d8
// (version v0.30.1-1-g19c79d8) on the database wkSeedSQL builds, at the clock
// wkClock (2026-11-16 ~16:40 UTC). They are minified: one line, no trailing
// newline, and only & < > " are escaped. Values that depend on the clock (the
// 7 and 30 day windows, the settings "last active" time of the current
// session) are rendered by the tests from exactly the captured inputs.
//
// Two names are cut from a page instead of being a whole page:
//
//   - login-shell is the login page up to its <main>: the document shell.
//   - dashboard-nav is the <nav> element of the dashboard page.
//
// Do not recompute a digest to make a test pass: it is the Rust server's page.
var goldenDigests = map[string]string{
	"confirm-delete-user":                 "dd12bbc5d0d324000f648ea7a9c6f3f43ae3bf34687289fae0573d427739fd38",
	"confirm-delete-user-wrong-password":  "5b018f71ce970ed9f1d69a2542392d786641befde03076846360d3246a5658e3",
	"confirm-exercise-delete-escaped":     "5f2a704e920014ef9adea4ddec1cc9178cf03b9ce6627d49fbb42a45bb203055",
	"confirm-logout-others":               "3715ced9cce4f0a34cbc252b6e87b4db70b0c351bed8439277128487d73157a6",
	"confirm-logout-others-none":          "d6b1e0cdf1ca6a556e9d44502d8484cd26d9fca5c56050d9bfa7ef6ff3c08738",
	"confirm-promote-user":                "2ee14377fd99540a789df3049e9e3bef939134b074b3c1c49d55897b6891b667",
	"confirm-promote-user-wrong-password": "344164a9482f71fc57271f9f24a62cbe872963c2faf19a70dbbd3c10c60d4deb",
	"dashboard":                           "16524f8de22f6ca6d74d1bb6bceb2c611c2d263cd3085aa6a175031c0d4de0f3",
	"dashboard-empty":                     "71025c84d49a26d7172940de1c4340edbadf471e2a337593ee02941e71dc3b4f",
	"dashboard-timezone-set":              "22006dc4ce442d6bf30bdc3fbcf621eed9731548548e9f87aa79ca5134162cdc",
	"exercises-edit":                      "00b3d2892a90028673f145d6a3c08b4f61e8808106dd43e0516d9de444cecbb5",
	"exercises-edit-error":                "d4882f26f6cee2e2de0bcf3a8042c9af695e55258644ab7510a78bd4d4a4e9b0",
	"exercises-edit-escaped":              "ad48f99386be214b7064884fd18722100714a71f258cebf9814a27a97988447e",
	"exercises-list":                      "148c64e4ea6a7a39fd306c63cfec829efcb9128eb38fde01eca82aa257b088be",
	"exercises-list-empty":                "f162c7131816f5454d6d4273e4ce67c016234320e8e60b1df45561d6dfc0e8bb",
	"exercises-new":                       "98b6d4ee073ede54989518144b63ab8234034dd0287ba374b09baae098929d26",
	"exercises-new-error":                 "6dc0e9aea3b9fbcb8d7b539a81ae75348f0a1e05d5f35aca28e1ad0a5b448dee",
	"login":                               "2ae37a8e79eb0b59dc7a0fb2cce99a750a59af0eda2452694e9ced548b487025",
	"login-failed":                        "8d95e4d0321c220b48eba70f57b39c650c9631f433b522d40e5aba0fb7dd1a3d",
	"settings":                            "41408b4854fd7e71a4daaf3814567482c6c450867e4db9bc0bf16bf4078a86c4",
	"settings-password-mismatch":          "01dfb7520cf735fa7370e9253069357a2d476d0e88fbd33977cc279c280ec1f8",
	"settings-timezone-error":             "e8d76a16e8a6df069c3a2d54ba5b2d77f7378dd08a56d39e3e67f7a1042f2f1d",
	"settings-timezone-set":               "95ad89ecfbc955d48f8a2505746c33e199ccb44dbafabf7747d3973527e41c00",
	"shared":                              "fabfd6ec0d34979317457c3dac422a936cdb149751965a91538a32b2a3c4aaea",
	"shared-forever":                      "1eb7076a5d609de56d798cdd7361ace8db402bb0a833d6210949146a5d421f5f",
	"stats-exercise-bench":                "0de41c8f58d8e44e54b464078df4d4eafc20749a0263aa2bc822c97984062f26",
	"stats-exercise-bob":                  "016df6e3cf33e19371ffa181c00b8dcdea61ebfa2a56e55ab2f599dbb57d6250",
	"stats-exercise-curl":                 "f67eb2f91e581b2e991a46e90a307d5bb363986bb7ecbd2ef0811939e84a45e6",
	"stats-exercise-deadlift":             "b5f421073e3e7ced421d46c21226513b752405c5e40fd59b9451684cceece72c",
	"stats-exercise-ohp":                  "c51d1fa5083efe66809521432923027fccd7c018012289bd1cfd7fdc5ab69045",
	"stats-exercise-plank":                "c28e92deb5d22767c2022e13c71b9deed33e3e1a64280b289bf662726bbc3a30",
	"stats-exercise-pullup":               "e1ac86ea434784d32768b68226f6dad64c1efa119cd4812a3bf3aa383da8cfcb",
	"stats-exercise-squat":                "bb25c37bdd7031fb2e5661b82cbcb9a8f11d2d1b6ebb77eac65bc026dcb80984",
	"stats-exercise-squat-e1rm-20":        "8c5968c7ccc9cf3347e00fb7313bd4a774e9fd5c1027cb88fc38bfcb87f1387a",
	"stats-exercise-squat-e1rm-all":       "f139012fbf46809d6ddbb383c36285f575aaf38be01f5174dfcb6b6c2fb63a61",
	"stats-exercise-squat-top_set-20":     "bb25c37bdd7031fb2e5661b82cbcb9a8f11d2d1b6ebb77eac65bc026dcb80984",
	"stats-exercise-squat-top_set-all":    "8065463542a391c91c696f59d1e0bed33a246dec8906fec7502df104ae84cb36",
	"stats-exercise-squat-unknown-query":  "bb25c37bdd7031fb2e5661b82cbcb9a8f11d2d1b6ebb77eac65bc026dcb80984",
	"stats-exercise-squat-volume-20":      "743767fe9c6cdd084b78b9211eb0d9f3bfacc3d2efbffa5131d937edd802201c",
	"stats-exercise-squat-volume-all":     "3d2cb99297cd7822fb677397c7fc1b4e8dd6c20f88a1a057e58c795d5300e7cb",
	"stats-index":                         "562e2778c7a073cf52ebfac5dcd1de3088ae67afb0c572bb18cc38f00e7a8c7f",
	"stats-index-empty":                   "f7777dde50ad257140000a4ef13bc7e88ee5136a8282d92806852f2fca915978",
	"stats-prs":                           "26aec50c533fc8ffbb665b46d4be3e731b8c6ab9dd4f7c6442b4c7643e403fac",
	"stats-prs-empty":                     "d47c0c45db98b23f94b5bb3ff36459d63ef3e6943963cbdd631c23764fb4b9cb",
	"users":                               "3fce26d1965d603999d1cb92067a7e2eadd5b87b357d95165405507bb2177152",
	"users-as-user":                       "36342ac510e555cf84b4bc085f12d5192d7318d1be019bad6ad393e85d17c332",
	"users-new":                           "715387ca6c20c1944d3d255dfe46308c184952e1d2024d762815516f2299ea3a",
	"users-new-weak":                      "90fc7db0f217c99176ddc3401c2ed8e6eaf7ca0a04c6fca10035bb131b94e97d",
	"workout-edit":                        "0cd2b247918464ae61b8aaf0da0603ef75a81e84a89046f126a0eb945b5c12dd",
	"workout-edit-log":                    "0e85e1fc261e7777e3a7caf5feee78e4c34f618602777e566756341293681fa3",
	"workout-edit-log-no-rpe":             "d30249bd66677ee340dc6a6bed16b4de05c269eb2832fd372a461d87d3522ac7",
	"workout-edit-no-notes":               "7aa6fd171642a1176c29cfd2e5338d5d0216c55ce91c6f0820b4c0a47e92df7f",
	"workout-show":                        "0e0209fbe614748533e352cf6cfc5f6006e8c2fada2e7dd7000d56ce370cba6b",
	"workout-show-bodyweight":             "9767974257b1bfa80030caf39c2e560f01833521a7cacfe4cd0692a381d6606f",
	"workout-show-empty":                  "6c147687f7fd0c2bf7ee259edecdb6871b66278ee9e3eb7bcee8532f6ebf82d6",
	"workout-show-notes":                  "597bd1080bf0ef0990e24ecd89ab091463f5776d5cfc048b9840be8846cadc1d",
	"workout-show-prefill":                "c97d5aa476e49d9400f523d68b1256eef6c465c4c9879708b4773a8df0b5b11b",
	"workout-show-prefill-foreign":        "0e0209fbe614748533e352cf6cfc5f6006e8c2fada2e7dd7000d56ce370cba6b",
	"workout-show-prefill-rpe":            "10ab75be7e8c95ebb51d636af4ae8505c35d45c317c3b555a8e6433802272a78",
	"workout-show-prefill-unknown":        "0e0209fbe614748533e352cf6cfc5f6006e8c2fada2e7dd7000d56ce370cba6b",
	"workout-show-shared":                 "76bb0d80f07d50eed720ddafc7c9e51187e0ae03bc444d0eeffcd2188876170b",
	"workout-show-shared-expired":         "3669d3d896ae06de429583a8c0783b5b4a7752a9e0f05fcc36aea296c3cabcbe",
	"workout-show-shared-forever":         "be281870e18c23b8f294c3d74cf02bc57c7975a1de6eecd3cc5cd2900b9589f7",
	"workout-show-timezone-set":           "e3a16b83a0ce66501fd25c24879cc0f7590251f57e36809b95cbb8bd10aac149",
	"workouts-list":                       "48a81129af52db763636cb7f9391291e81ef68a6ea9d9bd161e15cb1fc56c088",
	"workouts-list-empty":                 "0f975cf6143899a7cb779f2022e3ab862060c0b879fa891c976febb06f87feaf",
	"workouts-list-page-beyond":           "b0a11e5e3f9fd1067d2cb582a8e5b2b352adc5424e6785934d926d60c9a29cc1",
	"workouts-list-page-zero":             "48a81129af52db763636cb7f9391291e81ef68a6ea9d9bd161e15cb1fc56c088",
	"workouts-list-page2":                 "c8d4c3d42700a3a8c3ea2a00895df5c700f3e8b937f5145fdbf4f748af7f9175",
	"workouts-list-page3":                 "7eed6b033871e64a5d67d3ca4c20ee2f8aee39aa68c90d5701f9f13316746823",
	"workouts-new":                        "6b6a1f046f267627e1a89af90f2386595ba134bb523ad0cc67d93e76bf4fdcdd",
	"workouts-new-timezone-set":           "c00d3b3dc71d9e5f4503c306e9142a791c9b9a41e4eee6bb0bab98ad04995aa7",

	"dashboard-nav": "cbd553751a8bb6e7dac6265935cbc4deece3476fdfd952ce8bf7849de171575b",
	"login-shell":   "573ad79bae14aa386808b1d0989833f7233b9a0811698ef4deb6cd50c2112b5a",
}
