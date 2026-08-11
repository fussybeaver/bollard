use std::error::Error;
use std::fmt::{Display, Formatter};

use roxmltree::Document;
use url::Url;

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const MOBY_HOST: &str = "raw.githubusercontent.com";

#[derive(Debug, PartialEq, Eq)]
pub struct MobyInputSpec {
    pub url: String,
    pub reference: String,
}

#[derive(Debug)]
struct PomError(String);

impl Display for PomError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for PomError {}

pub fn parse_input_spec(contents: &str) -> Result<MobyInputSpec> {
    let document = Document::parse(contents)
        .map_err(|error| PomError(format!("could not parse pom.xml: {error}")))?;
    let input_specs: Vec<String> = document
        .descendants()
        .filter(|node| node.tag_name().name() == "inputSpec")
        .map(|node| node.text().unwrap_or_default().trim().to_string())
        .collect();

    let url = match input_specs.as_slice() {
        [] => return Err(pom_error("pom.xml does not contain an inputSpec element")),
        [url] if url.is_empty() => return Err(pom_error("pom.xml inputSpec is empty")),
        [url] => url.clone(),
        _ => return Err(pom_error("pom.xml contains multiple inputSpec elements")),
    };

    let parsed = Url::parse(&url)
        .map_err(|error| pom_error(format!("invalid pom.xml inputSpec URL: {error}")))?;
    if parsed.scheme() != "https" || parsed.host_str() != Some(MOBY_HOST) {
        return Err(pom_error(format!(
            "pom.xml inputSpec must use https://{MOBY_HOST}"
        )));
    }

    let segments: Vec<&str> = parsed
        .path_segments()
        .ok_or_else(|| pom_error("pom.xml inputSpec URL has no path"))?
        .collect();
    if segments.len() < 5 || segments[0] != "moby" || segments[1] != "moby" {
        return Err(pom_error(
            "pom.xml inputSpec URL must point to moby/moby",
        ));
    }

    let api_index = segments
        .windows(2)
        .position(|window| window == ["api", "docs"])
        .ok_or_else(|| pom_error("pom.xml inputSpec URL must point to api/docs"))?;
    if api_index <= 2 || api_index + 2 >= segments.len() {
        return Err(pom_error("pom.xml inputSpec URL has an invalid Moby ref"));
    }
    let reference = segments[2..api_index].join("/");

    Ok(MobyInputSpec { url, reference })
}

fn pom_error(message: impl Into<String>) -> Box<dyn Error> {
    Box::new(PomError(message.into()))
}

#[cfg(test)]
mod tests {
    use super::parse_input_spec;

    const POM: &str = r#"
        <project xmlns="http://maven.apache.org/POM/4.0.0">
          <build>
            <plugins>
              <plugin>
                <configuration>
                  <inputSpec>
                    https://raw.githubusercontent.com/moby/moby/docker-v29.4.1/api/docs/v1.53.yaml
                  </inputSpec>
                </configuration>
              </plugin>
            </plugins>
          </build>
        </project>
    "#;

    #[test]
    fn parses_moby_release_input_spec() {
        let input_spec = parse_input_spec(POM).unwrap();
        assert_eq!(input_spec.reference, "docker-v29.4.1");
        assert_eq!(
            input_spec.url,
            "https://raw.githubusercontent.com/moby/moby/docker-v29.4.1/api/docs/v1.53.yaml"
        );
    }

    #[test]
    fn accepts_namespaced_input_spec_elements() {
        let pom = POM
            .replace(
                "xmlns=\"http://maven.apache.org/POM/4.0.0\"",
                "xmlns=\"http://maven.apache.org/POM/4.0.0\" xmlns:pom=\"urn:test\"",
            )
            .replace("<inputSpec>", "<pom:inputSpec>")
            .replace("</inputSpec>", "</pom:inputSpec>");
        assert_eq!(parse_input_spec(&pom).unwrap().reference, "docker-v29.4.1");
    }

    #[test]
    fn rejects_missing_empty_duplicate_and_malformed_input_specs() {
        assert!(parse_input_spec("<project />").unwrap_err().to_string().contains("does not contain"));
        assert!(parse_input_spec("<project><inputSpec> </inputSpec></project>")
            .unwrap_err()
            .to_string()
            .contains("is empty"));
        assert!(parse_input_spec(
            "<project><inputSpec>a</inputSpec><inputSpec>b</inputSpec></project>"
        )
        .unwrap_err()
        .to_string()
        .contains("multiple"));
        assert!(parse_input_spec("<project><inputSpec>").unwrap_err().to_string().contains("parse"));
    }

    #[test]
    fn rejects_wrong_hosts_and_malformed_paths() {
        let wrong_host = POM.replace("raw.githubusercontent.com", "github.com");
        assert!(parse_input_spec(&wrong_host)
            .unwrap_err()
            .to_string()
            .contains("must use https://raw.githubusercontent.com"));

        let missing_api = POM.replace("api/docs", "swagger");
        assert!(parse_input_spec(&missing_api)
            .unwrap_err()
            .to_string()
            .contains("api/docs"));
    }

    #[test]
    fn accepts_branch_references_with_slashes() {
        let branch_pom = POM.replace("docker-v29.4.1", "fix/swagger-docs");
        assert_eq!(
            parse_input_spec(&branch_pom).unwrap().reference,
            "fix/swagger-docs"
        );
    }
}
