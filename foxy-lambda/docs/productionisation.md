# Steps to Build a Continuous Deployment (CD) Process for Production

This document outlines the steps required to transition the current staging setup into a fully functional Continuous Deployment (CD) pipeline for a production environment. It also highlights considerations for production readiness.

---

## 1. **Set Up a Separate Production Environment**
### Tasks:
- Duplicate the current CloudFormation (CF) templates and parameters for the staging (development) environment.
    - Update naming conventions and resource identifiers (e.g., User Pools, Identity Pools, Lambda functions).
    - Update the `STACK_NAME` and IAM user/policy names in the CF script to reflect the production environment.
- Deploy the updated CF templates to create production-specific resources:
  ```bash
  aws cloudformation deploy \
      --template-file production_resources.yaml \
      --stack-name ProductionIAMStack \
      --capabilities CAPABILITY_NAMED_IAM \
      --region <PRODUCTION_REGION>
  ```

## 2. **Configure AWS Lambda for Production**
### Tasks:
- Create a new Lambda function for production or deploy updates to an existing one.
- Update the Lambda function configuration:
    - **Environment Variables**:
        - Set production-specific environment variables (e.g., `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY`, `COGNITO_USER_POOL_ID`).
        - Use parameter stores like AWS Systems Manager (SSM) for sensitive data.
    - Update the `GitHub Actions` workflow to deploy to the production Lambda:
      ```yaml
      # Step 5: Deploy to AWS Lambda
      - name: Deploy to Production AWS Lambda
        run: |
          aws lambda update-function-code \
            --function-name foxy-backend-prod \
            --zip-file fileb://foxy-backend.zip
      ```

## 3. **Create a Production CI/CD Pipeline**
### Tasks:
- **GitHub Actions Workflow**:
    - Add a `production` branch to the workflow triggers:
      ```yaml
      on:
        push:
          branches:
            - production
      ```
    - Update the `role-to-assume` in the `configure-aws-credentials` step to the production-specific IAM role.

- Configure separate build artifacts for production (e.g., `foxy-backend-prod.zip`).
- Validate that the production pipeline builds, tests, and deploys independently of staging.

## 4. **Enhance Security**
### Tasks:
- **Service Account:**
    - Create a dedicated production service account using CloudFormation or manually.
    - Assign restricted permissions scoped to the production User Pool, Identity Pool, and Lambda function.
- **Access Keys:**
    - Store the production service account's access keys in a secure secret manager like AWS Secrets Manager or GitHub Secrets.
- **Environment Separation:**
    - Ensure staging and production use separate AWS accounts or isolated environments to avoid accidental overlaps.
- **Monitoring and Alerts:**
    - Enable AWS CloudWatch metrics and alarms for Lambda functions.
    - Configure CloudTrail to monitor IAM and Cognito activities.

## 5. **Testing and Validation**
### Tasks:
- Before activating the production environment:
    - Validate the configuration of all resources using AWS Management Console or CLI.
    - Perform integration testing using a test user and a dedicated test API Gateway endpoint.
    - Verify the token issuance process and authentication flows for both users and service accounts.

## 6. **Switch DNS and Go Live**
### Tasks:
- If applicable, update the DNS records to point to the production API Gateway.
- Test end-to-end flows in the production environment with real users.
- Monitor logs for unexpected behaviors or errors.

---

## Additional Considerations
- **Rollback Plan:** Ensure a rollback strategy is defined for each deployment step.
    - Use versioned Lambda deployments for easy reverts.
    - Backup the production CF stack before major changes.
- **Cost Management:** Monitor costs using AWS Budgets, especially for resources like Lambda and Cognito.
- **Documentation:**
    - Maintain detailed documentation for the production setup, including resource ARNs, IAM policies, and pipeline configurations.

By following these steps, the application can be transitioned to a robust and secure production environment with a streamlined CI/CD process.
