# frozen_string_literal: true

module Inkoc
  module AST
    module Predicates
      def optional?
        false
      end

      def identifier?
        false
      end

      def string?
        false
      end

      def self?
        false
      end

      def constant?
        false
      end

      def import?
        false
      end

      def method?
        false
      end

      def variable_definition?
        false
      end

      def expression?
        true
      end

      def return?
        false
      end

      def keyword_argument?
        false
      end

      def throw?
        false
      end

      def block_type?
        false
      end

      def lambda_type?
        false
      end

      def lambda_or_block_type?
        false
      end

      def block?
        false
      end

      def block_without_signature?
        false
      end

      def lambda?
        false
      end

      def closure?
        false
      end

      def self_type?
        false
      end

      def never_type?
        false
      end

      def send?
        false
      end

      def global?
        false
      end

      def object?
        false
      end

      def trait?
        false
      end

      def reassign_attribute?
        false
      end

      def attribute?
        false
      end

      def trait_implementation?
        false
      end

      def cast?
        false
      end

      def define_attribute?
        false
      end
    end
  end
end
