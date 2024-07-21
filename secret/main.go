package main

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
	"os"

	"cloud.google.com/go/compute/metadata"
	secretmanager "cloud.google.com/go/secretmanager/apiv1"
	"cloud.google.com/go/secretmanager/apiv1/secretmanagerpb"
	"github.com/aliyun/aliyun-oss-go-sdk/oss"
)

type Config struct {
	CHAIN_ID         string `json:"CHAIN_ID"`
	ENDPOINT         string `json:"ENDPOINT"`
	CONTRACT_ADDRESS string `json:"CONTRACT_ADDRESS"`
	DATA_ADDRESS     string `json:"DATA_ADDRESS"`
	GATE_ADDRESS     string `json:"GATE_ADDRESS"`
	WALLET_SK        string `json:"WALLET_SK"`
}

func OnAliCloud() bool {
	resp, err := http.Get("http://100.100.100.200/latest/meta-data/instance-id")
	if err != nil {
		return false
	}
	resp.Body.Close()
	return true
}

func WithSecurityToken(t string) oss.ClientOption {
	return func(c *oss.Client) {
		c.Config.SecurityToken = t
	}
}

func main() {
	if metadata.OnGCE() {
		walletSk := "wallet-sk"
		// context
		ctx := context.Background()
		projectNumber, err := metadata.NumericProjectID()
		if err != nil {
			log.Fatalf("Failed to get project number: %v", err)
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
			log.Fatalf("failed to write secret to file %+v", err)
		}

	} else if OnAliCloud() {
		resp, err := http.Get("http://100.100.100.200/latest/meta-data/region-id")
		if err != nil {
			log.Fatalf("failed to read metadata %+v", err)
		}
		defer resp.Body.Close()
		regionId, err := io.ReadAll(resp.Body)
		if err != nil {
			log.Fatalf("failed to read region %+v", err)
		}

		resp, err = http.Get("http://100.100.100.200/latest/meta-data/ram/security-credentials/aizel-inference")
		if err != nil {
			log.Fatalf("failed to read metadata %+v", err)
		}
		defer resp.Body.Close()
		credential, err := io.ReadAll(resp.Body)
		if err != nil {
			log.Fatalf("failed to read region %+v", err)
		}
		var accessConfig oss.Config
		err = json.Unmarshal(credential, &accessConfig)
		if err != nil {
			log.Fatalf("failed to parse access config %+v", err)
		}
		log.Printf("%v", accessConfig)
		bucketName := "aizel-tdx"
		objectName := "tdx-data.json"
		client, err := oss.New(fmt.Sprintf("oss-%s.aliyuncs.com", regionId), accessConfig.AccessKeyID, accessConfig.AccessKeySecret, WithSecurityToken(accessConfig.SecurityToken))
		if err != nil {
			log.Fatalf("failed to create oss client: %v+", err)
		}
		bucket, err := client.Bucket(bucketName)
		if err != nil {
			log.Fatalf("failed to get bucket: %v+", err)
		}
		body, err := bucket.GetObject(objectName)
		if err != nil {
			log.Fatalf("failed to get object: %v+", err)
		}
		data, err := io.ReadAll(body)
		if err != nil {
			log.Fatalf("failed to read data: %v+", err)
		}
		homeDir, err := os.UserHomeDir()
		if err != nil {
			log.Fatalf("failed to get home dir: %v", err)
		}
		err = os.WriteFile(fmt.Sprintf("%s/config", homeDir), data, 0644)
		if err != nil {
			log.Fatalf("failed to write secret to file %+v", err)
		}

		var config Config
		err = json.Unmarshal(data, &config)
		if err != nil {
			log.Fatalf("failed to parse data: %v+", err)
		}

		err = os.WriteFile(fmt.Sprintf("%s/wallet-sk", homeDir), []byte(config.WALLET_SK), 0644)
		if err != nil {
			log.Fatalf("failed to write secret to file %+v", err)
		}
	} else {
		log.Fatalf("Only support on Google Cloud and AliCloud")
	}
}
