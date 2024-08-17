package main

import (
	"context"
	"encoding/base64"
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

func GetSecret(skName string) {
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
	accessRequest := &secretmanagerpb.AccessSecretVersionRequest{
		Name: fmt.Sprintf("projects/%s/secrets/%s/versions/latest", projectNumber, skName),
	}
	result, err := client.AccessSecretVersion(ctx, accessRequest)
	if err != nil {
		log.Fatalf("failed to access secret: %v", err)
	}
	homeDir, err := os.UserHomeDir()
	if err != nil {
		log.Fatalf("failed to get home dir: %v", err)
	}
	decodedData := make([]byte, base64.StdEncoding.DecodedLen(len(result.Payload.Data)))
	_, err = base64.StdEncoding.Decode(decodedData, result.Payload.Data)
	if err != nil {
		log.Fatalf("failed to decode the payload data: %v", err)
	}
	printable := []byte{}
	for _, b := range decodedData {
		if (b >= 32 && b <= 126) || b == '\n' || b == '\r' || b == '\t' {
			printable = append(printable, b)
		}
	}
	err = os.WriteFile(fmt.Sprintf("%s/aizel/aizel_config.yml", homeDir), printable, 0644)
	if err != nil {
		log.Fatalf("failed to write secret to file %+v", err)
	}
}

func main() {
	if metadata.OnGCE() {
		GetSecret("aizel-config")
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
		objectName := "aizel_config.yml"
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
		err = os.WriteFile(fmt.Sprintf("%s/aizel/aizel_config.yml", homeDir), data, 0644)
		if err != nil {
			log.Fatalf("failed to write secret to file %+v", err)
		}
	} else {
		log.Fatalf("Only support on Google Cloud and AliCloud")
	}
}
