#!coding: utf-8
import json

from errno import ESRCH

import yaml
import re
import subprocess
import os

from datetime import datetime
from time import time, sleep

from subprocess import Popen
from subprocess import PIPE
from subprocess import CalledProcessError
from subprocess import TimeoutExpired
from argparse import ArgumentParser
from qcloud_cos import CosConfig
from qcloud_cos import CosS3Client
import sys
import logging

from qcloud_cos.cos_exception import CosException, CosServiceError

failures = 0
passed = 0
index = {'Contents': []}


def load_config():
    with open('perfs.yaml', 'r') as f:
        data = yaml.load(f, Loader=yaml.FullLoader)
        return data


conf = load_config()


def build_COSclient(secretID, secretKey, Region, Endpoint):
    logging.basicConfig(level=logging.INFO, stream=sys.stdout)
    secret_id = secretID
    secret_key = secretKey
    region = Region
    token = None  # TODO(zhihanz) support token for client
    scheme = 'http'
    config = CosConfig(Region=region, SecretId=secret_id, SecretKey=secret_key, Token=token, Scheme=scheme,
                       Domain=Endpoint)
    client = CosS3Client(config)
    return client


def execute(suit, bin_path, host, port, concurrency, iterations, output_dir, type, region, bucket, S3path, secretID,
            secretKey, endpoint, rerun):
    base_cfg = conf['config']
    if iterations == "":
        iterations = suit.get("iterations", base_cfg['iterations'])
    if concurrency == "":
        concurrency = suit.get("concurrency", base_cfg['concurrency'])
    if bin_path == "":
        logging.warning("you should specific path for fuse-benchmark binary file")
        return
    suit_name = re.sub(r"\s+", '-', suit['name'])
    file_name = "{}-result.json".format(suit_name)
    json_path = os.path.join(output_dir, file_name)
    S3key = os.path.join(S3path, file_name)
    if type == "COS":
        if rerun == "False":
            COScli = build_COSclient(secretID, secretKey, region, endpoint)
            try :
                response = COScli.head_object(
                        Bucket=bucket,
                        Key=S3key
                    )
            except CosServiceError as e:
                if e.get_error_code() == 'NoSuchResource':
                    logging.info("continue on test")
                else:
                    logging.info("other issue occured, {}".format(e.get_error_code()))
            except ConnectionError as ce:
                logging.info("timeout for {}".format(S3key))
            else :
                # S3 key exists in given bucket just return
                logging.info("S3 key {} found in bucket and not continue on it".format(S3key))
                return
    command = '{} -c {} -i {} -h {} -p {} --query "{}" --json "{}" '.format(bin_path, concurrency, iterations, host,
                                                                            port, suit['query'], json_path)
    logging.warning("perf {}, query: {} \n".format(suit_name, suit['query']))

    proc = Popen(command, shell=True, env=os.environ)
    start_time = datetime.now()
    while proc.poll() is None:
        sleep(0.01)
    total_time = (datetime.now() - start_time).total_seconds()
    if type == "COS":
        COScli = build_COSclient(secretID, secretKey, region, endpoint)
        with open(json_path, 'rb') as fp:
            response = COScli.put_object(
                Bucket=bucket,
                Body=fp,
                Key=S3key,
                StorageClass='STANDARD',
                EnableMD5=False
            )
            index['Contents'].append({
                'path':  S3key,
                'file_name': file_name,
            })
            logging.warning(response['ETag'])

    global failures
    global passed

    if proc.returncode is None:
        try:
            proc.kill()
        except OSError as e:
            if e.errno != ESRCH:
                raise

        failures += 1
    elif proc.returncode != 0:
        failures += 1
    else:
        passed += 1


if __name__ == '__main__':
    parser = ArgumentParser(description='fuse perf tests')
    parser.add_argument('-o', '--output', default=".", help='Perf results directory')
    parser.add_argument('-b', '--bin', default="fuse-benchmark", help='Fuse benchmark binary')
    parser.add_argument('--host', default="127.0.0.1", help='Clickhouse handler Server host')
    parser.add_argument('-p', '--port', default="9001", help='Clickhouse handler Server port')
    parser.add_argument('-c', '--concurrency', default="", help='Set default concurrency for all perf tests')
    parser.add_argument('-i', '--iteration', default="",
                        help='Set default iteration number for each performance tests to run')
    parser.add_argument('-t', '--type', default="local",
                        help='Set storage endpoint for performance testing, support local and COS')
    parser.add_argument('--region', default="", help='Set storage region')
    parser.add_argument('--bucket', default="", help='Set storage bucket')
    parser.add_argument('--path', default="", help='Set absolute path to store objects')
    parser.add_argument('--secretID', default="", help='Set storage secret ID')
    parser.add_argument('--secretKey', default="", help='Set storage secret Key')
    parser.add_argument('--endpoint', default="", help='Set storage endpoint')
    parser.add_argument('--rerun', default="False", help='if rerun set as true, it will rerun all perfs.yaml completely')
    args = parser.parse_args()

    for suit in conf['perfs']:
        execute(suit, args.bin, args.host, args.port, args.concurrency, args.iteration, args.output,
                args.type, args.region, args.bucket, args.path, args.secretID, args.secretKey, args.endpoint,
                args.rerun)
    with open(os.path.join(args.output, "index.json"), 'w') as outfile:
        json.dump(index, outfile)
    s3 = build_COSclient(args.secretID, args.secretKey, args.region, args.endpoint)
    with open(os.path.join(args.output, "index.json"), 'rb') as fp:
        response = s3.put_object(
            Bucket=args.bucket,
            Body=fp,
            Key=os.path.join(args.path, "index.json"),
            StorageClass='STANDARD',
            EnableMD5=False
        )
        logging.warning(response['ETag'])
