//! Pins the CloudFormation template to the code: the table's keys, the
//! bucket's protections, the bucket policy and the exact actions of the
//! managed policy. No stand-in enforces a bucket policy or IAM, so these
//! pins are the only check of them before a deployment.
#![cfg(test)]

use serde_json::{Value, json};

const TEMPLATE: &str = include_str!("../../../infra/flare-stores.template.json");

fn template() -> Value {
    serde_json::from_str(TEMPLATE).unwrap()
}

fn resource(name: &str) -> Value {
    template()["Resources"][name].clone()
}

#[test]
fn the_template_defines_exactly_the_table_the_bucket_its_policy_and_the_managed_policy() {
    let resources = template()["Resources"].as_object().unwrap().clone();
    let mut names: Vec<&str> = resources.keys().map(String::as_str).collect();
    names.sort_unstable();
    assert_eq!(
        names,
        [
            "FilesBucket",
            "FilesBucketPolicy",
            "FlareStoresPolicy",
            "SharesTable"
        ]
    );
    assert_eq!(resources["SharesTable"]["Type"], "AWS::DynamoDB::Table");
    assert_eq!(resources["FilesBucket"]["Type"], "AWS::S3::Bucket");
    assert_eq!(
        resources["FilesBucketPolicy"]["Type"],
        "AWS::S3::BucketPolicy"
    );
    assert_eq!(
        resources["FlareStoresPolicy"]["Type"],
        "AWS::IAM::ManagedPolicy"
    );
    assert_eq!(
        template()["Outputs"]["PolicyArn"]["Value"],
        json!({"Ref": "FlareStoresPolicy"})
    );
}

#[test]
fn the_table_has_string_keys_pk_and_sk_and_no_index() {
    let table = resource("SharesTable");
    let properties = &table["Properties"];
    assert_eq!(
        properties["KeySchema"],
        json!([
            {"AttributeName": "pk", "KeyType": "HASH"},
            {"AttributeName": "sk", "KeyType": "RANGE"}
        ])
    );
    assert_eq!(
        properties["AttributeDefinitions"],
        json!([
            {"AttributeName": "pk", "AttributeType": "S"},
            {"AttributeName": "sk", "AttributeType": "S"}
        ])
    );
    assert!(properties.get("GlobalSecondaryIndexes").is_none());
    assert!(properties.get("LocalSecondaryIndexes").is_none());
    assert_eq!(properties["TableName"], json!({"Ref": "TableName"}));
}

#[test]
fn the_table_deletes_expired_login_codes_through_its_ttl_attribute() {
    let table = resource("SharesTable");
    assert_eq!(
        table["Properties"]["TimeToLiveSpecification"],
        json!({"AttributeName": "expires_at", "Enabled": true})
    );
}

#[test]
fn the_table_bills_on_demand_keeps_point_in_time_recovery_and_resists_deletion() {
    let table = resource("SharesTable");
    let properties = &table["Properties"];
    assert_eq!(properties["BillingMode"], "PAY_PER_REQUEST");
    assert_eq!(
        properties["PointInTimeRecoverySpecification"]["PointInTimeRecoveryEnabled"],
        true
    );
    assert_eq!(properties["DeletionProtectionEnabled"], true);
    assert_eq!(table["DeletionPolicy"], "Retain");
    assert_eq!(table["UpdateReplacePolicy"], "Retain");
}

#[test]
fn the_bucket_blocks_public_access_disables_acls_encrypts_and_is_kept() {
    let bucket = resource("FilesBucket");
    let properties = &bucket["Properties"];
    assert_eq!(
        properties["PublicAccessBlockConfiguration"],
        json!({
            "BlockPublicAcls": true,
            "BlockPublicPolicy": true,
            "IgnorePublicAcls": true,
            "RestrictPublicBuckets": true
        })
    );
    assert_eq!(
        properties["OwnershipControls"]["Rules"],
        json!([{"ObjectOwnership": "BucketOwnerEnforced"}])
    );
    assert_eq!(
        properties["BucketEncryption"]["ServerSideEncryptionConfiguration"],
        json!([{"ServerSideEncryptionByDefault": {"SSEAlgorithm": "AES256"}}])
    );
    assert!(properties.get("VersioningConfiguration").is_none());
    assert!(properties.get("WebsiteConfiguration").is_none());
    assert_eq!(bucket["DeletionPolicy"], "Retain");
    assert_eq!(bucket["UpdateReplacePolicy"], "Retain");
}

fn statement(policy: &Value, sid: &str) -> Value {
    policy["PolicyDocument"]["Statement"]
        .as_array()
        .unwrap()
        .iter()
        .find(|statement| statement["Sid"] == sid)
        .unwrap_or_else(|| panic!("no statement {sid}"))
        .clone()
}

#[test]
fn the_bucket_policy_denies_requests_without_tls_and_puts_without_if_none_match() {
    let policy = resource("FilesBucketPolicy")["Properties"].clone();
    assert_eq!(
        policy["PolicyDocument"]["Statement"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    let tls = statement(&policy, "DenyWithoutTls");
    assert_eq!(tls["Effect"], "Deny");
    assert_eq!(tls["Principal"], "*");
    assert_eq!(tls["Action"], "s3:*");
    assert_eq!(
        tls["Condition"],
        json!({"Bool": {"aws:SecureTransport": "false"}})
    );
    let put = statement(&policy, "DenyPutWithoutIfNoneMatch");
    assert_eq!(put["Effect"], "Deny");
    assert_eq!(put["Action"], "s3:PutObject");
    assert_eq!(
        put["Condition"],
        json!({"Null": {"s3:if-none-match": "true"}})
    );
}

#[test]
fn the_managed_policy_allows_exactly_the_actions_the_stores_call() {
    let policy = resource("FlareStoresPolicy")["Properties"].clone();
    let statements = policy["PolicyDocument"]["Statement"].as_array().unwrap();
    assert_eq!(statements.len(), 3);
    assert!(
        statements
            .iter()
            .all(|statement| statement["Effect"] == "Allow")
    );
    let table = statement(&policy, "SharesTable");
    assert_eq!(
        table["Action"],
        json!([
            "dynamodb:GetItem",
            "dynamodb:Query",
            "dynamodb:PutItem",
            "dynamodb:UpdateItem",
            "dynamodb:DeleteItem",
            "dynamodb:ConditionCheckItem",
            "dynamodb:DescribeTable"
        ])
    );
    assert_eq!(
        table["Resource"],
        json!({"Fn::GetAtt": ["SharesTable", "Arn"]})
    );
    let objects = statement(&policy, "FilesObjects");
    assert_eq!(objects["Action"], json!(["s3:GetObject", "s3:PutObject"]));
    assert_eq!(
        objects["Resource"],
        json!({"Fn::Sub": "${FilesBucket.Arn}/files/*"})
    );
    let bucket = statement(&policy, "FilesBucket");
    assert_eq!(bucket["Action"], json!(["s3:ListBucket"]));
    assert_eq!(
        bucket["Resource"],
        json!({"Fn::GetAtt": ["FilesBucket", "Arn"]})
    );
}
