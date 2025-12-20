use tonic::transport::Channel;
use tonic::Streaming; // Import Streaming agar dikenali

// Module ini men-generate struct dari file .proto
pub mod financial_proto {
    tonic::include_proto!("financial");
}

use financial_proto::{AnalyzeRequest, AnalyzeResponse}; 

#[derive(Clone)]
pub struct GrpcClient {
    pub client: financial_proto::financial_extractor_client::FinancialExtractorClient<Channel>,
}

impl GrpcClient {
    pub async fn connect(dst: String) -> Result<Self, tonic::transport::Error> {
        let client = financial_proto::financial_extractor_client::FinancialExtractorClient::connect(dst).await?;
        Ok(Self { client })
    }

    pub async fn analyze_stream(
        &self,
        file_bytes: Vec<u8>,
        extension: String,
        filename: String,
        mode: String, 
    ) -> Result<Streaming<AnalyzeResponse>, Box<dyn std::error::Error + Send + Sync>> { // <--- WAJIB: Tambah + Send + Sync
        
        let request = tonic::Request::new(AnalyzeRequest {
            file_name: filename,
            file_content: file_bytes,
            file_extension: extension,
            analyze_mode: mode, 
        });

        let mut client = self.client.clone();
        
        // Kita harus mapping error tonic agar eksplisit menjadi Box<dyn Error + Send + Sync>
        let response = client.extract_and_analyze(request).await.map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(e);
            boxed
        })?;
        
        Ok(response.into_inner())
    }
}