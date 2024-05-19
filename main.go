package main

import (
	"crypto/rsa"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net/http"

	"github.com/golang-jwt/jwt/v4"
)

type JsonWebKeySet struct {
	Keys []JsonWebKey `json:"keys"`
}

type JsonWebKey struct {
	Alg string `json:"alg"`
	Kty string `json:"kty"`
	N   string `json:"n"`
	Use string `json:"use"`
	Kid string `json:"kid"`
	E   string `json:"e"`
}

const (
	ExpectedIssuer = "https://confidentialcomputing.googleapis.com"
	WellKnownPath  = "/.well-known/openid-configuration"
)

type OpenidConfiguration map[string]interface{}

func getJsonWebKeySetUri() (string, error) {
	httpClient := http.Client{}
	resp, err := httpClient.Get(ExpectedIssuer + WellKnownPath)
	if err != nil {
		return "", fmt.Errorf("failed to get openid configuration %v ", err)
	}
	configuration, err := io.ReadAll(resp.Body)
	if err != nil {
		return "", fmt.Errorf("failed to read response %v ", err)
	}
	conf := OpenidConfiguration{}
	err = json.Unmarshal(configuration, &conf)
	if err != nil {
		return "", fmt.Errorf("failed to unmarshal response %v ", err)
	}
	return conf["jwks_uri"].(string), nil
}

func getJsonWebKeySet() (*JsonWebKeySet, error) {
	uri, err := getJsonWebKeySetUri()
	if err != nil {
		return nil, err
	}
	httpClient := http.Client{}
	resp, err := httpClient.Get(uri)
	if err != nil {
		return nil, fmt.Errorf("failed to get json web set %v ", err)
	}
	jwksBytes, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, fmt.Errorf("failed to read response %v ", err)
	}
	jwks := &JsonWebKeySet{}
	if err = json.Unmarshal(jwksBytes, jwks); err != nil {
		return nil, fmt.Errorf("failed to unmarshal json web key set %v ", jwks)
	}
	return jwks, nil
}

func base64DecodeUint(s string) (*big.Int, error) {
	b, err := base64.RawURLEncoding.DecodeString(s)
	if err != nil {
		return nil, err
	}
	z := new(big.Int)
	z.SetBytes(b)
	return z, nil
}

func GetRSAPublicKey(t *jwt.Token) (any, error) {
	issuer := t.Header["iss"]
	if issuer != ExpectedIssuer {
		return nil, fmt.Errorf("unkown issuer, expected %s ", ExpectedIssuer)
	}
	jsonWebKeySet, err := getJsonWebKeySet()
	if err != nil {
		return nil, fmt.Errorf("failed to get json web key set")
	}
	kid := t.Header["kid"]
	for _, key := range jsonWebKeySet.Keys {
		if key.Kid != kid {
			continue
		}
		n, err := base64DecodeUint(key.N)
		if err != nil {
			return nil, fmt.Errorf("failed to decode key.N %v ", err)
		}
		e, err := base64DecodeUint(key.E)
		if err != nil {
			return nil, fmt.Errorf("failed to decode key.E %v ", err)
		}
		return &rsa.PublicKey{
			N: n,
			E: int(e.Int64()),
		}, nil
	}

	return nil, fmt.Errorf("failed to find key with kid %s ", kid)
}

func UnmarshalToken(tokenBytes []byte) (*jwt.Token, error) {

	token, err := jwt.NewParser().Parse(string(tokenBytes), GetRSAPublicKey)
	if err != nil {
		return nil, fmt.Errorf("failed to parse json web token %v ", err)
	}
	var ve *jwt.ValidationError
	if errors.As(err, &ve) {
		if ve.Errors&(jwt.ValidationErrorExpired) != 0 {
			return nil, fmt.Errorf("token is expired %v ", ve)
		}
		return nil, fmt.Errorf("failed to verify json web token %v ", ve)
	}
	return token, nil
}