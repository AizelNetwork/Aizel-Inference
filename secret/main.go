package main

import (
	"context"
	"fmt"
	"log"
	"os"

	"cloud.google.com/go/compute/metadata"
	secretmanager "cloud.google.com/go/secretmanager/apiv1"
	"cloud.google.com/go/secretmanager/apiv1/secretmanagerpb"
)

func main() {
	var projectNumber string
	walletSk := "wallet-sk"
	var err error
	// context
	ctx := context.Background()

	if metadata.OnGCE() {
		projectNumber, err = metadata.NumericProjectID()
		if err != nil {
			log.Fatalf("Failed to get project number: %v", err)
		}
	} else {
		log.Fatalf("Only support on GCP cloud engine")
	}

	// create secretmanager client
	client, err := secretmanager.NewClient(ctx)
	if err != nil {
		log.Fatalf("failed to setup client: %v", err)
	}

	// get token first
	accessRequest := &secretmanagerpb.AccessSecretVersionRequest{
		Name: fmt.Sprintf("projects/%s/secrets/%s/versions/latest", projectNumber, walletSk),
	}

	result, err := client.AccessSecretVersion(ctx, accessRequest)
	if err != nil {
		log.Fatalf("failed to access secret: %v", err)
	}
	homeDir, err := os.UserHomeDir()
	if err != nil {
		log.Fatalf("failed to get home dir: %v", err)
	}
	err = os.WriteFile(fmt.Sprintf("%s/wallet-sk", homeDir), result.Payload.Data, 0644)
	if err != nil {
		log.Fatalf("failed to write k8s token to file %+v", err)
	}
}
