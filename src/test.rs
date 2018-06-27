#[cfg(test)]
use serde_json;
#[cfg(test)]
use container::{Container, ContainerInfo};
#[cfg(test)]
use process::{Top};
#[cfg(test)]
use stats::{Stats, StatsReader};
#[cfg(test)]
use system::SystemInfo;
#[cfg(test)]
use image::Image;
#[cfg(test)]
use filesystem::FilesystemChange;
#[cfg(test)]
use version::Version;
#[cfg(test)]
use hyper::client::response::Response;
#[cfg(test)]
use util::MemoryStream;
#[cfg(test)]
use hyper::Url;
#[cfg(test)]
use hyper::http::h1::{Http11Message, HttpWriter};
#[cfg(test)]
use std::io::Write;

#[test]
#[cfg(test)]
fn get_containers() {
    let response = get_containers_response();
    let _: Vec<Container> = match serde_json::from_str(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_stats_single() {
    let response = get_stats_single_event(1);

    print!("{}", response);
    let _: Stats = match serde_json::from_str(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_stats_streaming() {
    let url = Url::parse("http://localhost/who/cares").unwrap();
    let stream = MemoryStream::with_input(get_stats_response().as_bytes());
    let message = Http11Message::with_stream(Box::new(stream));
    let response = Response::with_message(url, Box::new(message)).unwrap();
    let mut reader = StatsReader::new(response);

    let stats = reader.next().unwrap().unwrap();
    assert_eq!(stats.read, "2015-04-09T07:02:08.480022081Z".to_string());

    let stats = reader.next().unwrap().unwrap();
    assert_eq!(stats.read, "2015-04-09T07:02:08.480022082Z".to_string());

    let stats = reader.next().unwrap().unwrap();
    assert_eq!(stats.read, "2015-04-09T07:02:08.480022083Z".to_string());

    assert!(reader.next().unwrap().is_err());
}

#[test]
#[cfg(test)]
fn get_system_info() {
    let response = get_system_info_response();
    let _: SystemInfo = match serde_json::from_str(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_images() {
    let response = get_images_response();
    let images : Vec<Image> = serde_json::from_str(&response).unwrap();
    assert_eq!(3, images.len());
}

#[test]
#[cfg(test)]
fn get_container_info() {
    let response = get_container_info_response();
    serde_json::from_str::<ContainerInfo>(&response).unwrap();
}

#[test]
#[cfg(test)]
fn get_processes() {
    let response = get_processes_response();
    let _: Top = match serde_json::from_str(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_filesystem_changes() {
    let response = get_filesystem_changes_response();
    let _: Vec<FilesystemChange> = match serde_json::from_str(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[test]
#[cfg(test)]
fn get_version(){
    let response = get_version_response();
    let _: Version = match serde_json::from_str(&response) {
        Ok(body) => body,
        Err(_) => { assert!(false); return; }
    };
}

#[cfg(test)]
fn get_containers_response() -> String {
    return "[{\"Id\":\"ed3221f4adc05b9ecfbf56b1aa76d4e6e70d5b73b3876c322fc10d017c64ca86\",\"Names\":[\"/rust\"],\"Image\":\"ghmlee/rust:latest\",\"Command\":\"bash\",\"Created\":1439434052,\"Ports\":[{\"IP\":\"0.0.0.0\",\"PrivatePort\":8888,\"PublicPort\":8888,\"Type\":\"tcp\"}],\"SizeRootFs\":253602755,\"Labels\":{},\"Status\":\"Exited (137) 12 hours ago\",\"HostConfig\":{\"NetworkMode\":\"default\"},\"SizeRw\":10832473}]".to_string();
}

#[cfg(test)]
fn get_containers_response_long() -> String {
    return "[{\"Id\":\"2931241fd8d910316faaff849d906b0decfcd1ec123fff5153bcbae9f73a112e\",\"Names\":[\"/abdcdev_abdcctl_1\"],\"Image\":\"abcdefgh/abcdef-abcd-abdcctl:abcdefgh-v0.1.0\",\"ImageID\":\"96e385a9d743afd5b704a794f92560cce88aee8ab464018b1a84caf7b5d9d22b\",\"Command\":\"/w/w npm start\",\"Created\":1449220146,\"Ports\":[{\"PrivatePort\":3000,\"Type\":\"tcp\"},{\"IP\":\"127.0.0.1\",\"PrivatePort\":3100,\"PublicPort\":3100,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"dbb2feac5036db2f484344e8732207404b4d0ae22540cf43f4344fc17db71d90\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"abdcctl\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 2 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"8344036afa76f538144939b1b5c21535c0c297965c9edb1c50ded6c9de0d4e51\",\"Names\":[\"/abdcdev_abdcopsabcd_1\"],\"Image\":\"abcdefgh/abcdef-abcd-ops:abcdefgh-v0.1.0\",\"ImageID\":\"bfd4a651b7bb5bcdebe77fc68e2723194d40ad2beb9c3e33e4649f1b411fce9b\",\"Command\":\"/w/w npm start\",\"Created\":1449220138,\"Ports\":[{\"PrivatePort\":3000,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"210b124c721f2d3da1d0ca81217b2044e4185852a3759ff140d26067eca1a258\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"abdcopsabcd\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 2 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"eb4c97482c961833c5bc0ce7e727bf68c1d6e853f777da4ef32ad1da0cd00551\",\"Names\":[\"/abdcdev_abcdefadmin_1\"],\"Image\":\"abdcdev_abcdefadmin\",\"ImageID\":\"c2d23cb88f87426007f8179af74a6964a06b69d5911c4dab0e3e5b9acaabd6af\",\"Command\":\"/w/w npm start\",\"Created\":1449220133,\"Ports\":[{\"PrivatePort\":3000,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"408798c1a8a0ba71883180c19806262f411654c530e53f7d1c5f77f769e64e2e\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"abcdefadmin\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 2 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"b28afbd208625505fd68ab3814c4d14f7a1bd70db1a68d5ef8a2f0af046eed55\",\"Names\":[\"/abdcdev_abcdefctl_1\"],\"Image\":\"abcdefgh/abcdef-abcdefctl:abcdefgh-v0.1.3\",\"ImageID\":\"8dd0d7a8f2b161b423e3c66410ffb01c46cff76782e41e7b7633cced0ae696ef\",\"Command\":\"/usr/bin/abcdefctl supervise\",\"Created\":1449220030,\"Ports\":[],\"Labels\":{\"com.docker.compose.config-hash\":\"454b184fb1d39a1716f888389d4932c6a583c28f895cbc77c1b8d2b291910219\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"abcdefctl\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\"},\"Status\":\"Up 2 hours\",\"HostConfig\":{\"NetworkMode\":\"host\"}},{\"Id\":\"552b7f8975fe4306749eb9e058366e7001c34523b776c5200c366295a8e12f31\",\"Names\":[\"/weaveproxy\"],\"Image\":\"weaveworks/weaveexec:1.3.1\",\"ImageID\":\"619d88f027004f82d23c1bd2a93636cde7e9dd8d0306b63801c6c5504828c8fa\",\"Command\":\"/home/weave/weaveproxy --no-default-ipalloc --no-rewrite-hosts --without-dns -H /var/run/weave/weave.sock -H 0.0.0.0:12345 --tlsverify --tlscacert /home/weave/tls/ca.pem --tlscert /home/weave/tls/cert.pem --tlskey /home/weave/tls/key.pem\",\"Created\":1449150621,\"Ports\":[],\"Labels\":{\"works.weave.role\":\"system\"},\"Status\":\"Up 22 hours\",\"HostConfig\":{\"NetworkMode\":\"host\"}},{\"Id\":\"8dec1642c907035b1afdd765a3960134edb72bab7ec21221f1104e6c592b9d47\",\"Names\":[\"/weave\"],\"Image\":\"weaveworks/weave:1.3.1\",\"ImageID\":\"4482abc1ac8c5c36e464a8ff222b72f77c46c73295507b1d6681c59af1e0794e\",\"Command\":\"/home/weave/weaver --port 6783 --name 4e:d0:a7:51:27:54 --nickname dev --datapath weave --iface veth-weave --ipalloc-range 169.254.0.0/16 --dns-effective-listen-address 172.17.0.1 --dns-listen-address 172.17.0.1:53 --http-addr 127.0.0.1:6784 --docker-api unix:///var/run/docker.sock\",\"Created\":1449150612,\"Ports\":[],\"Labels\":{\"works.weave.role\":\"system\"},\"Status\":\"Up 22 hours\",\"HostConfig\":{\"NetworkMode\":\"host\"}},{\"Id\":\"a281d40eb5a5c549dd627297f3546a762d07e3c06585f101d426111c5ca87125\",\"Names\":[\"/abdcdev_abcdefabcd_1\"],\"Image\":\"abcdefgh/abcdef-abcd\",\"ImageID\":\"45d74b4cf11517e380fbc52196ba12dc6c0f83fe97ab1cb9ada4c92d0cadad89\",\"Command\":\"/w/w npm start\",\"Created\":1449146597,\"Ports\":[{\"IP\":\"127.0.0.1\",\"PrivatePort\":3000,\"PublicPort\":3200,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"0bfc03b9fc73d9ff97d329a55d2121225d97feb7efdb50bd32e3a00ee8fa16b6\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"abcdefabcd\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"698bdfb90a2bf91fd5a5717a362110617e07348c8d841a3fcaf52c6001f6b925\",\"Names\":[\"/abdcdev_sslrproxy_1\"],\"Image\":\"abdcdev_sslrproxy\",\"ImageID\":\"383fb91a69460529844cff4a8af20c56054461f148d5b168d66e60cb32e4de1a\",\"Command\":\"/w/w nginx -c /etc/nginx/nginx.conf\",\"Created\":1449146521,\"Ports\":[{\"IP\":\"0.0.0.0\",\"PrivatePort\":443,\"PublicPort\":443,\"Type\":\"tcp\"},{\"PrivatePort\":80,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"e00af6e792687216c18e4972ff8211940fde66ac94535ff50a1a7bd1880ae61d\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"sslrproxy\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"2bc7e3061892492c82054fa3be6c6e5e9f8ca9f933b217385bcfc3d55ac9bd1d\",\"Names\":[\"/abdcdev_adminabcdefauth_1\"],\"Image\":\"abcdefgh/abcdef-auth\",\"ImageID\":\"de0c2e6c590f0c50870520728b1a1b1a943ebee0f6f38eac10b26117bdc01cd1\",\"Command\":\"/w/w npm start\",\"Created\":1449146518,\"Ports\":[{\"PrivatePort\":3001,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"9990f233869e71289783868219004d1006bcb61062eeb96ec06b6591293c57b6\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"adminabcdefauth\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"41b2d1ba3a3135756562c777cf02822a4f81cbd24daf535920083f73b1e7496d\",\"Names\":[\"/abdcdev_intabcdefauth_1\"],\"Image\":\"abcdefgh/abcdef-auth\",\"ImageID\":\"de0c2e6c590f0c50870520728b1a1b1a943ebee0f6f38eac10b26117bdc01cd1\",\"Command\":\"/w/w npm start\",\"Created\":1449146515,\"Ports\":[{\"PrivatePort\":3001,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"3bfd1573ed9b17830b71a7d1ca5badc28623af9366a503146c2038a17e4e3795\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"intabcdefauth\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"603fcc4e5c90d8430d87cf4a5d17ed286eec71d7f118495c55bc8c2b9f503552\",\"Names\":[\"/abdcdev_ldap_1\"],\"Image\":\"abdcdev_ldap\",\"ImageID\":\"b53cb44bbfb3cdbcc0e2d6361e840d4e54bbe86af6bcfc458632a38448f5e06e\",\"Command\":\"/w/w /bin/sh -c /usr/local/bin/start_ldap.sh\",\"Created\":1449146451,\"Ports\":[{\"IP\":\"127.0.0.1\",\"PrivatePort\":389,\"PublicPort\":389,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"1737e5571ef796e1b0bd5280e719b159d893510bc93cc41b841f625954cea7e0\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"ldap\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"0\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"997b74083f92e52396d0743f8c69d5665dc525194d7a3e14d67cdd9ea09c4375\",\"Names\":[\"/abdcdev_rsyslog_1\"],\"Image\":\"abcdefgh/rsyslog:elasticsearch\",\"ImageID\":\"465c2dc38907b52a4f6ddc6ba02793cd0be1fc87d59e9d68cdc37811706c9149\",\"Command\":\"/w/w /usr/local/bin/start.sh -n\",\"Created\":1449146448,\"Ports\":[{\"IP\":\"127.0.0.1\",\"PrivatePort\":514,\"PublicPort\":1514,\"Type\":\"tcp\"},{\"PrivatePort\":514,\"Type\":\"udp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"80fcc75ec55280a51b7091c25dc549d274a02789a0461b5239507b6b735b4285\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"rsyslog\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"1\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"411e267eeae94f73de1b63623314f93d5f1df971420ff8076b9f85407389ef58\",\"Names\":[\"/abdcdev_kibana_1\"],\"Image\":\"abcdefgh/kibana\",\"ImageID\":\"3e305ce8fe94240b42c4ac215c1e266d265e093cdbdee7154df2d8be0bd22445\",\"Command\":\"/w/w /bin/sh -c 'counter=0    while [ ! \\\\\"$(curl elasticsearch:9200 > /dev/null)\\\\\" -a $counter -lt 30  ]; do    sleep 1; ((counter++)); echo $counter;    done    ./bin/kibana'\",\"Created\":1449146425,\"Ports\":[{\"IP\":\"127.0.0.1\",\"PrivatePort\":5601,\"PublicPort\":5601,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"7b4ba8414c065d3aa206cecd8bbc114b0de6da1acabb55eec1819609fed60cbb\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"kibana\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"1\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"6af6d7cfeae2745fb0aca72ca29a87196f835464244717680322d84af4df3605\",\"Names\":[\"/abdcdev_ssllogstash_1\"],\"Image\":\"abdcdev_ssllogstash\",\"ImageID\":\"8a6111f837f79cdeb81139de5621b666d68d91fe82df37456b7d1118e90608d7\",\"Command\":\"/w/w /opt/logstash/bin/logstash -f /etc/logstash/conf.d/\",\"Created\":1449146297,\"Ports\":[{\"IP\":\"0.0.0.0\",\"PrivatePort\":50000,\"PublicPort\":514,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"bbd9a7ddc4471ab02c1b716e597b9b1d07c8a278b3b3daef4e420f54f1c60e82\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"ssllogstash\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"b444aa95b3f2ac55d188e3b8bcde5d262dddb9a3bd860f5ec7f502f283e61e19\",\"Names\":[\"/abdcdev_extsaltmaster_1\"],\"Image\":\"abdcdev_extsaltmaster\",\"ImageID\":\"818e61f5ab40dcbc322350fcc35ceb38c0142b3706aca74758397ac375e75ab3\",\"Command\":\"/w/w salt-master -l info\",\"Created\":1449146294,\"Ports\":[{\"IP\":\"0.0.0.0\",\"PrivatePort\":44506,\"PublicPort\":44506,\"Type\":\"tcp\"},{\"IP\":\"0.0.0.0\",\"PrivatePort\":44505,\"PublicPort\":44505,\"Type\":\"tcp\"},{\"PrivatePort\":4506,\"Type\":\"tcp\"},{\"PrivatePort\":4505,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"c164e56fef0af4c9323ec77e45f715b279e245b267b85848f48a5f6d0fb58cc8\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"extsaltmaster\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"ef0bf8c1ef2b1b13cdb16f3d8bd4dc65542519e88c7a91584eedc6ca0dded456\",\"Names\":[\"/ef0bf8c1ef_abdcdev_netopsabcd_1\"],\"Image\":\"abcdefgh/abcdef-abcd-netops:abcdefgh-v0.2.0\",\"ImageID\":\"697f78b7255da1a5f422dc863023d50011d1c3256771e56f240adf3c63e4ea32\",\"Command\":\"/w/w npm start\",\"Created\":1449146288,\"Ports\":[{\"PrivatePort\":3000,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"39a290b43d5c490055274d13a1939434ac44d1f898eadbb32759f262a6e2bb8d\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"netopsabcd\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 2 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"82240f370bc8770a589ffb71c9af0b1ba551c46bb81fbc6edb008405e0c84e03\",\"Names\":[\"/abdcdev_netopsjobs_1\"],\"Image\":\"abdcdev_netopsjobs\",\"ImageID\":\"1f84baea8e9a805346f755cd8cb095ad3ba13cfd00dd3a24fb55e049d46ebde5\",\"Command\":\"/w/w npm start\",\"Created\":1449146120,\"Ports\":[{\"PrivatePort\":3000,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"8c426b26c9e6de3b8af33fb5159b8b442440cfa96775bacc65394c828a0b2250\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"netopsjobs\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"ad8f4a8344710568f4955f08b649ac1ee1d033a23e5395701b4f0a81fc50d05b\",\"Names\":[\"/abdcdev_rabbitmq_1\"],\"Image\":\"rabbitmq:3-management\",\"ImageID\":\"9e6ba0accabe2633011ce5d4b5f9da4531c4df5358033b63c182f83161255d89\",\"Command\":\"/w/w /docker-entrypoint.sh rabbitmq-server\",\"Created\":1449146117,\"Ports\":[{\"IP\":\"127.0.0.1\",\"PrivatePort\":5672,\"PublicPort\":5672,\"Type\":\"tcp\"},{\"PrivatePort\":5671,\"Type\":\"tcp\"},{\"PrivatePort\":4369,\"Type\":\"tcp\"},{\"PrivatePort\":25672,\"Type\":\"tcp\"},{\"IP\":\"127.0.0.1\",\"PrivatePort\":15672,\"PublicPort\":15672,\"Type\":\"tcp\"},{\"PrivatePort\":15671,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"ccc4a223ccc863d037e375dec1325e81d3b2cfaeb2bd565eb2466eb5aaddd0c4\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"rabbitmq\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"78237fdec313691858e76c19507a967b14acbbd3e308907706ede436c9ff795f\",\"Names\":[\"/abdcdev_iperf3_1\"],\"Image\":\"abcdefgh/iperf3:latest\",\"ImageID\":\"78c515c71c616bf59ac082e7453a9d787e00c3f1c409e0f0cf5bd86292086f2f\",\"Command\":\"/bin/sh -c 'iperf3 -s'\",\"Created\":1449146093,\"Ports\":[{\"IP\":\"0.0.0.0\",\"PrivatePort\":5201,\"PublicPort\":5201,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"437e525f28fdc0fba960139dfa70e85ce73cfb9caaf5d5680ee72da906710c9d\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"iperf3\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"2\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}},{\"Id\":\"de4ae2223b7732780c44e6395550487ece0abb4776f38c1fe5c317706bd54f32\",\"Names\":[\"/abdcdev_elasticsearch_1\"],\"Image\":\"abcdefgh/elasticsearch\",\"ImageID\":\"750277ad2cea8c5bc712abaa3962bc8eed78944cec19bda9663c65e7f0bd5d6a\",\"Command\":\"/w/w /elasticsearch/bin/elasticsearch\",\"Created\":1449146051,\"Ports\":[{\"IP\":\"127.0.0.1\",\"PrivatePort\":9300,\"PublicPort\":9300,\"Type\":\"tcp\"},{\"IP\":\"127.0.0.1\",\"PrivatePort\":9200,\"PublicPort\":9200,\"Type\":\"tcp\"}],\"Labels\":{\"com.docker.compose.config-hash\":\"60b131b015ee9d7fed7774b654838b6ad37904de9192d8be082ef101e13d9e0d\",\"com.docker.compose.container-number\":\"1\",\"com.docker.compose.oneoff\":\"False\",\"com.docker.compose.project\":\"abdcdev\",\"com.docker.compose.service\":\"elasticsearch\",\"com.docker.compose.version\":\"1.5.1\",\"za.co.abcdefgh.abcdef.projectname\":\"abcdefDev\",\"za.co.abcdefgh.abcdef.startorder\":\"0\"},\"Status\":\"Up 4 hours\",\"HostConfig\":{\"NetworkMode\":\"default\"}}]".to_string();
}

#[cfg(test)]
fn get_system_info_response() -> String {
    return "{\"Containers\":6,\"Debug\":0,\"DockerRootDir\":\"/var/lib/docker\",\"Driver\":\"btrfs\",\"DriverStatus\":[[\"Build Version\",\"Btrfs v3.17.1\"],[\"Library Version\",\"101\"]],\"ExecutionDriver\":\"native-0.2\",\"ID\":\"WG63:3NIU:TSI2:FV7J:IL2O:YPXA:JR3F:XEKT:JZVR:JA6T:QMYE:B4SB\",\"IPv4Forwarding\":1,\"Images\":190,\"IndexServerAddress\":\"https://index.docker.io/v1/\",\"InitPath\":\"/usr/libexec/docker/dockerinit\",\"InitSha1\":\"30c93967bdc3634b6036e1a76fd547bbe171b264\",\"KernelVersion\":\"3.18.6\",\"Labels\":null,\"MemTotal\":16854257664,\"MemoryLimit\":1,\"NCPU\":4,\"NEventsListener\":0,\"NFd\":68,\"NGoroutines\":95,\"Name\":\"core\",\"OperatingSystem\":\"CoreOS 607.0.0\",\"RegistryConfig\":{\"IndexConfigs\":{\"docker.io\":{\"Mirrors\":null,\"Name\":\"docker.io\",\"Official\":true,\"Secure\":true}},\"InsecureRegistryCIDRs\":[\"127.0.0.0/8\"]},\"SwapLimit\":1}".to_string();
}

#[cfg(test)]
fn get_images_response() -> String {
    return "[{\"Created\":1428533761,\"Id\":\"533da4fa223bfbca0f56f65724bb7a4aae7a1acd6afa2309f370463eaf9c34a4\",\"ParentId\":\"84ac0b87e42afe881d36f03dea817f46893f9443f9fc10b64ec279737384df12\",\"RepoTags\":[\"ghmlee/rust:nightly\"],\"Size\":0,\"VirtualSize\":806688288},{\"Created\":1371157430,\"Id\":\"511136ea3c5a64f264b78b5433614aec563103b4d4702f3ba7d4d2698e22c158\",\"ParentId\":\"\",\"RepoTags\":[],\"Size\":0,\"VirtualSize\":0},
    {\"Created\":1371157430,\"Id\":\"511136ea3c5a64f264b78b5433614aec563103b4d4702f3ba7d4d2698e22c158\",\"ParentId\":\"\",\"RepoTags\":null,\"Size\":0,\"VirtualSize\":0}]".to_string();
}

#[cfg(test)]
fn get_container_info_response() -> String {
    return r#"{"Id":"774758ca1db8d05bd848d2b3456c8253a417a0511329692869df1cbe82978d37","Created":"2016-10-25T11:59:37.858589354Z","Path":"rails","Args":["server","-b","0.0.0.0"],"State":{"Status":"running","Running":true,"Paused":false,"Restarting":false,"OOMKilled":false,"Dead":false,"Pid":13038,"ExitCode":0,"Error":"","StartedAt":"2016-10-25T11:59:38.261828009Z","FinishedAt":"0001-01-01T00:00:00Z"},"Image":"sha256:f5e9d349e7e5c0f6de798d732d83fa5e087695cd100149121f01c891e6167c13","ResolvConfPath":"/var/lib/docker/containers/774758ca1db8d05bd848d2b3456c8253a417a0511329692869df1cbe82978d37/resolv.conf","HostnamePath":"/var/lib/docker/containers/774758ca1db8d05bd848d2b3456c8253a417a0511329692869df1cbe82978d37/hostname","HostsPath":"/var/lib/docker/containers/774758ca1db8d05bd848d2b3456c8253a417a0511329692869df1cbe82978d37/hosts","LogPath":"/var/lib/docker/containers/774758ca1db8d05bd848d2b3456c8253a417a0511329692869df1cbe82978d37/774758ca1db8d05bd848d2b3456c8253a417a0511329692869df1cbe82978d37-json.log","Name":"/railshello_web_1","RestartCount":0,"Driver":"aufs","MountLabel":"","ProcessLabel":"","AppArmorProfile":"","ExecIDs":null,"HostConfig":{"Binds":[],"ContainerIDFile":"","LogConfig":{"Type":"json-file","Config":{}},"NetworkMode":"railshello_default","PortBindings":{"3000/tcp":[{"HostIp":"","HostPort":"3000"}]},"RestartPolicy":{"Name":"","MaximumRetryCount":0},"AutoRemove":false,"VolumeDriver":"","VolumesFrom":[],"CapAdd":null,"CapDrop":null,"Dns":null,"DnsOptions":null,"DnsSearch":null,"ExtraHosts":null,"GroupAdd":null,"IpcMode":"","Cgroup":"","Links":null,"OomScoreAdj":0,"PidMode":"","Privileged":false,"PublishAllPorts":false,"ReadonlyRootfs":false,"SecurityOpt":null,"UTSMode":"","UsernsMode":"","ShmSize":67108864,"Runtime":"runc","ConsoleSize":[0,0],"Isolation":"","CpuShares":0,"Memory":0,"CgroupParent":"","BlkioWeight":0,"BlkioWeightDevice":null,"BlkioDeviceReadBps":null,"BlkioDeviceWriteBps":null,"BlkioDeviceReadIOps":null,"BlkioDeviceWriteIOps":null,"CpuPeriod":0,"CpuQuota":0,"CpusetCpus":"","CpusetMems":"","Devices":null,"DiskQuota":0,"KernelMemory":0,"MemoryReservation":0,"MemorySwap":0,"MemorySwappiness":-1,"OomKillDisable":false,"PidsLimit":0,"Ulimits":null,"CpuCount":0,"CpuPercent":0,"IOMaximumIOps":0,"IOMaximumBandwidth":0},"GraphDriver":{"Name":"aufs","Data":null},"Mounts":[],"Config":{"Hostname":"774758ca1db8","Domainname":"","User":"","AttachStdin":false,"AttachStdout":false,"AttachStderr":false,"ExposedPorts":{"3000/tcp":{}},"Tty":false,"OpenStdin":false,"StdinOnce":false,"Env":["RACK_ENV=development","PROJECT_NAME=rails_hello","GLOBAL_PASSWORD=magic","SOME_PASSWORD=secret","RAILS_ENV=development","DATABASE_URL=postgres://postgres@db:5432/rails_hello_development","PATH=/usr/local/bundle/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin","RUBY_MAJOR=2.3","RUBY_VERSION=2.3.1","RUBY_DOWNLOAD_SHA256=b87c738cb2032bf4920fef8e3864dc5cf8eae9d89d8d523ce0236945c5797dcd","RUBYGEMS_VERSION=2.6.7","BUNDLER_VERSION=1.13.4","GEM_HOME=/usr/local/bundle","BUNDLE_PATH=/usr/local/bundle","BUNDLE_BIN=/usr/local/bundle/bin","BUNDLE_SILENCE_ROOT_WARNING=1","BUNDLE_APP_CONFIG=/usr/local/bundle"],"Cmd":["rails","server","-b","0.0.0.0"],"Image":"faraday/rails_hello","Volumes":null,"WorkingDir":"/usr/src/app","Entrypoint":null,"OnBuild":null,"Labels":{"com.docker.compose.config-hash":"ff040c76ba24b1bac8d89e95cfb5ba7e29bd19423ed548a1436ae3c94bc6381a","com.docker.compose.container-number":"1","com.docker.compose.oneoff":"False","com.docker.compose.project":"railshello","com.docker.compose.service":"web","com.docker.compose.version":"1.8.1","io.fdy.cage.lib.coffee_rails":"/usr/src/app/vendor/coffee-rails","io.fdy.cage.pod":"frontend","io.fdy.cage.shell":"bash","io.fdy.cage.srcdir":"/usr/src/app","io.fdy.cage.target":"development","io.fdy.cage.test":"bundle exec rake"}},"NetworkSettings":{"Bridge":"","SandboxID":"ca243185e052f364f6f9e4141ee985397cda9c66a87258f8a8048a05452738cf","HairpinMode":false,"LinkLocalIPv6Address":"","LinkLocalIPv6PrefixLen":0,"Ports":{"3000/tcp":[{"HostIp":"0.0.0.0","HostPort":"3000"}]},"SandboxKey":"/var/run/docker/netns/ca243185e052","SecondaryIPAddresses":null,"SecondaryIPv6Addresses":null,"EndpointID":"","Gateway":"","GlobalIPv6Address":"","GlobalIPv6PrefixLen":0,"IPAddress":"","IPPrefixLen":0,"IPv6Gateway":"","MacAddress":"","Networks":{"railshello_default":{"IPAMConfig":null,"Links":null,"Aliases":["web","774758ca1db8"],"NetworkID":"4b237b1de0928a11bb399adaa249705b666bdc5dece3e9bdc260a630643bf945","EndpointID":"7d5e1e9df4bdf400654b96afdd1d42040c150a4f5b414f084c8fd5c95a9a906e","Gateway":"172.24.0.1","IPAddress":"172.24.0.3","IPPrefixLen":16,"IPv6Gateway":"","GlobalIPv6Address":"","GlobalIPv6PrefixLen":0,"MacAddress":"02:42:ac:18:00:03"}}}}"#.to_string();
}

#[cfg(test)]
fn get_processes_response() -> String {
    return "{\"Processes\":[[\"4586\",\"999\",\"rust\"]],\"Titles\":[\"PID\",\"USER\",\"COMMAND\"]}".to_string();
}

#[cfg(test)]
fn get_filesystem_changes_response() -> String {
    return "[{\"Path\":\"/tmp\",\"Kind\":0}]".to_string();
}

#[cfg(test)]
fn get_version_response() -> String {
    return "{\"Version\":\"1.8.1\",\"ApiVersion\":\"1.20\",\"GitCommit\":\"d12ea79\",\"GoVersion\":\"go1.4.2\",\"Os\":\"linux\",\"Arch\":\"amd64\",\"KernelVersion\":\"4.0.9-boot2docker\",\"BuildTime\":\"Thu Aug 13 02:49:29 UTC 2015\"}".to_string();
}

#[cfg(test)]
fn get_stats_response() -> String {
    let headers = "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: Close\r\n\r\n";
    let s1 = get_stats_single_event(1);
    let s2 = get_stats_single_event(2);
    let s3 = get_stats_single_event(3);

    let stream = MemoryStream::with_input(headers.as_bytes());
    let mut writer = HttpWriter::ChunkedWriter(stream);
    writer.write(s1.as_bytes());
    writer.write(b"\n");
    writer.write(s2.as_bytes());
    writer.write(b"\n");
    writer.write(s3.as_bytes());

    let buf = match writer.end() {
        Ok(w) => w,
        Err(_) => { panic!("error ending writer for stats response"); }
    };
    let body = String::from_utf8(buf.into_inner()).unwrap();
    return body
}

#[cfg(test)]
fn get_stats_single_event(n: u64) -> String {
    return format!("{{\"read\":\"2015-04-09T07:02:08.48002208{}Z\",\"networks\":{{}},\"cpu_stats\":{{\"cpu_usage\":{{\"total_usage\":19194125000,\"percpu_usage\":[14110113138,3245604417,845722573,992684872],\"usage_in_kernelmode\":1110000000,\"usage_in_usermode\":18160000000}},\"system_cpu_usage\":1014488290000000,\"throttling_data\":{{\"periods\":0,\"throttled_periods\":0,\"throttled_time\":0}}}},\"memory_stats\":{{\"usage\":208437248,\"max_usage\":318791680,\"stats\":{{\"active_anon\":27213824,\"active_file\":129069056,\"cache\":178946048,\"hierarchical_memory_limit\":18446744073709551615,\"hierarchical_memsw_limit\":18446744073709551615,\"inactive_anon\":0,\"inactive_file\":49876992,\"mapped_file\":10809344,\"pgfault\":99588,\"pgmajfault\":819,\"pgpgin\":130731,\"pgpgout\":153466,\"rss\":29331456,\"rss_huge\":6291456,\"total_active_anon\":27213824,\"total_active_file\":129069056,\"total_cache\":178946048,\"total_inactive_anon\":0,\"total_inactive_file\":49876992,\"total_mapped_file\":10809344,\"total_pgfault\":99588,\"total_pgmajfault\":819,\"total_pgpgin\":130731,\"total_pgpgout\":153466,\"total_rss\":29331456,\"total_rss_huge\":6291456,\"total_unevictable\":0,\"total_writeback\":0,\"unevictable\":0,\"writeback\":0}},\"limit\":16854257664}},\"blkio_stats\":{{\"io_service_bytes_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"Read\",\"value\":150687744}},{{\"major\":8,\"minor\":0,\"op\":\"Write\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Sync\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Async\",\"value\":150687744}},{{\"major\":8,\"minor\":0,\"op\":\"Total\",\"value\":150687744}}],\"io_serviced_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"Read\",\"value\":484}},{{\"major\":8,\"minor\":0,\"op\":\"Write\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Sync\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Async\",\"value\":484}},{{\"major\":8,\"minor\":0,\"op\":\"Total\",\"value\":484}}],\"io_queue_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"Read\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Write\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Sync\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Async\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Total\",\"value\":0}}],\"io_service_time_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"Read\",\"value\":2060941295}},{{\"major\":8,\"minor\":0,\"op\":\"Write\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Sync\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Async\",\"value\":2060941295}},{{\"major\":8,\"minor\":0,\"op\":\"Total\",\"value\":2060941295}}],\"io_wait_time_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"Read\",\"value\":5476872825}},{{\"major\":8,\"minor\":0,\"op\":\"Write\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Sync\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Async\",\"value\":5476872825}},{{\"major\":8,\"minor\":0,\"op\":\"Total\",\"value\":5476872825}}],\"io_merged_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"Read\",\"value\":79}},{{\"major\":8,\"minor\":0,\"op\":\"Write\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Sync\",\"value\":0}},{{\"major\":8,\"minor\":0,\"op\":\"Async\",\"value\":79}},{{\"major\":8,\"minor\":0,\"op\":\"Total\",\"value\":79}}],\"io_time_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"\",\"value\":1814}}],\"sectors_recursive\":[{{\"major\":8,\"minor\":0,\"op\":\"\",\"value\":294312}}]}}}}", n).to_string();
}
