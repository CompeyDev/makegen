use regex::Regex;
use serde::Deserialize;
use std::{fmt::Write, fs};
use toml::{self, value::Array, Value};

const MAKEFILE_COMMENT_HEADER: &str = r#"# WARNING:
# This file is automatically generated by makegen.
# It is not intended for manual editing.

"#;

const MAKEFILE_HEADER: &str = r#"check_defined = \
$(strip $(foreach 1,$1, \
    $(call __check_defined,$1,$(strip $(value 2)))))
__check_defined = \
$(if $(value $1),, \
    $(error Mandatory argument $1$(if $2, ($2))$(if $(value @), \
            not provided. Required by target `$@`)))

log_prefix := \x1b[34m[\u001b[0m\x1b[31m*\x1b[34m\x1b[34m]\u001b[0m
command_prefix := \x1b[34m[\u001b[0m\x1b[31m\#\x1b[34m\x1b[34m]\u001b[0m

"#;

// Future QoL updates will include schema checks before parsing the
// config ;)

// struct Variable {
//     required: bool,
//     description: String
// }

// struct Step {
//     log: String,
//     command: String
// }

// #[derive(Deserialize, Debug, Clone)]
// struct Target {
//     variables: Variable,
//     steps: Array<Step>,
// }

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
struct Config {
    windows: toml::value::Table,
    linux: toml::value::Table,
}

fn main() {
    let config_contents = fs::read_to_string("./makegen.toml")
        .expect("No config found in current working directory, aborting.");

    let config: Config = toml::from_str(config_contents.as_str())
        .expect("Failed to parse config. Does it follow proper syntax?");

    #[cfg(target_os = "windows")]
    let runtime_config = &config.windows.clone();

    #[cfg(target_os = "linux")]
    let runtime_config = &config.linux.clone();

    let mut makefile_contents = String::new();
    let mut pre_target_variables_check = String::new();

    for (target, steps_values) in runtime_config {
        match steps_values.get("variables") {
            Some(table) => {
                construct_variable_checks(table, &mut pre_target_variables_check);
                construct_steps(steps_values, Some(table), &mut makefile_contents);
            }
            None => {
                construct_steps(steps_values, None, &mut makefile_contents);
            }
        };

        makefile_contents.insert_str(0, format!("{}\n", pre_target_variables_check).as_str());
        makefile_contents.insert_str(0, format!("\r{}:\n", target).as_str());
    }

    makefile_contents.insert_str(0, MAKEFILE_HEADER);
    makefile_contents.insert_str(0, MAKEFILE_COMMENT_HEADER);

    fs::write("./Makefile", makefile_contents)
        .expect("failed to write Makefile contents. Aborting.");
}

fn construct_steps(
    steps_values: &Value,
    variables_array: Option<&Value>,
    makefile_contents: &mut String,
) {
    let steps_iter: Array = steps_values
        .get("steps")
        .expect("expected array `steps`, found `None`")
        .as_array()
        .expect("failed to cast `Value` to `Array` (make sure that `steps` is of the right kind)")
        .to_owned();

    for step in steps_iter {
        let formatted = format!(
            "	@echo -e \"${{log_prefix}} {}\"\n	@echo -e \"${{command_prefix}} {}\"\n	@{}",
            step.get("log")
                .expect("expected required value `log` for table `step`"),
            step.get("command")
                .expect("expected required value `command` for table `step`"),
            step.get("command")
                .expect("expected required value `command` for table `step`")
                .as_str()
                .unwrap()
        );

        match variables_array {
            Some(array) => {
                // match of this pattern would include the brackets
                let pat =
                    Regex::new(r"\((.*?)\)").expect("failed to construct step variable matcher");

                let matches: Vec<String> = pat
                    .find_iter(formatted.as_str())
                    .filter_map(|any| any.as_str().parse().ok())
                    .collect();

                for instance in matches {
                    // we need to trim the bounding brackets
                    let mut var_usage_instance_chars = instance.chars();
                    var_usage_instance_chars.next();
                    var_usage_instance_chars.next_back();

                    let var_usage_instance = var_usage_instance_chars.as_str();

                    let available_vars = array
                        .as_array()
                        .expect("expected type `Array` for `variables`")[0]
                        .as_table()
                        .expect("expected type `Table` for variables list");

                    let var_is_predefined = available_vars.contains_key(var_usage_instance);

                    if !var_is_predefined {
                        panic!("variable {} used before definition!", var_usage_instance);
                    }
                }
            }
            None => (),
        }

        write!(makefile_contents, "{}\n", formatted).expect("real");
    }
}

fn construct_variable_checks(variables_table: &Value, variable_checks: &mut String) {
    let variables_iter: Array = variables_table
        .as_array()
        .expect("expected variables table to contain array!")
        .to_owned();

    // we need to cast Value into toml::value::Table
    for var in variables_iter {
        // I really love O(n^2). So cool.
        // If you have a better way of doing this, please lmk.
        let var_meta = match var {
            Value::Table(table) => table,
            _ => panic!(
                "out of bounds value type! Does the configuration follow the required syntax?"
            ),
        };

        for (variable_name, variable_details) in var_meta {
            if variable_details
                .get("required")
                .expect("expected required value `required` for table `variable`")
                .as_bool()
                .unwrap()
                == true
            {
                write!(
                    variable_checks,
                    "	@:$(call check_defined, {}, {})\n",
                    variable_name,
                    variable_details
                        .get("description")
                        .expect("expected required value `description` for table `variable`")
                )
                .expect("failed to write to variable_checks stream! Aborting.");
            }
        }
    }
}
